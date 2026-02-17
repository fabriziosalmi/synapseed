"""OpenAI-compatible LLM client for local and remote models.

Supports Ollama, LM Studio, OpenRouter, and any OpenAI-compatible endpoint.
Configuration via .env or environment variables.
"""

from __future__ import annotations

import logging
import os
import re
import subprocess
import time
from dataclasses import dataclass, field

from openai import OpenAI

log = logging.getLogger(__name__)


def strip_think_blocks(text: str) -> str:
    """Remove <think>...</think> reasoning blocks from model output.

    Some models (Qwen3, DeepSeek-R1) emit chain-of-thought inside
    <think> tags.  We strip these so scoring sees only the final answer.
    Handles unclosed <think> blocks (model hit token limit mid-thought).
    """
    # Closed blocks
    text = re.sub(r"<think>.*?</think>", "", text, flags=re.DOTALL)
    # Unclosed block (model ran out of tokens while thinking)
    text = re.sub(r"<think>.*$", "", text, flags=re.DOTALL)
    return text.strip()


# ── System prompt variants for benchmark ablation ────────────────────
SYSTEM_PROMPT_GENERIC = "You are a helpful coding assistant."

SYSTEM_PROMPT_OPTIMIZED = (
    "You are a precise code analysis assistant. Follow these rules strictly:\n"
    "1. Only mention files, symbols, and code patterns that you are certain exist.\n"
    "2. When citing code, use exact file paths and symbol names.\n"
    "3. Be specific about numbers — count carefully before stating quantities.\n"
    "4. If unsure about something, say so rather than guessing.\n"
    "5. Structure your answer: first answer the question directly, then provide details.\n"
    "6. Do not invent or hallucinate file paths, function names, or code you haven't seen."
)

NOTHINK_INSTRUCTION = (
    "\nDo not use <think> reasoning blocks. Answer directly and concisely."
)


def build_system_prompt(optimized: bool = False, think: bool = True) -> str:
    """Build a system prompt with optional optimization and think control."""
    base = SYSTEM_PROMPT_OPTIMIZED if optimized else SYSTEM_PROMPT_GENERIC
    if not think:
        base += NOTHINK_INSTRUCTION
    return base


@dataclass
class LLMResponse:
    """Structured response from an LLM call."""

    content: str
    tokens_prompt: int
    tokens_completion: int
    latency_s: float
    model: str
    success: bool = True
    error: str | None = None

    @property
    def tokens_total(self) -> int:
        return self.tokens_prompt + self.tokens_completion


@dataclass
class LLMClient:
    """OpenAI-compatible client with dual-endpoint failover.

    Supports two LM Studio endpoints for load balancing / failover.
    If the primary fails, automatically retries on the secondary.

    Usage::

        client = LLMClient.from_env()
        resp = client.chat("What is the Router struct?")

        # Run across all configured models:
        for client in LLMClient.all_from_env():
            resp = client.chat("...")
    """

    base_url: str = "http://localhost:11434/v1"
    api_key: str = "ollama"
    base_url_2: str | None = None
    api_key_2: str | None = None
    model: str = "qwen/qwen3-1.7b"
    temperature: float = 0.0
    timeout: int = 900
    max_retries: int = 2
    max_tokens: int = 0  # 0 = unlimited, let the model decide

    _client: OpenAI = field(init=False, repr=False)
    _client_2: OpenAI | None = field(init=False, repr=False, default=None)

    def __post_init__(self):
        self._client = OpenAI(base_url=self.base_url, api_key=self.api_key)
        if self.base_url_2:
            self._client_2 = OpenAI(
                base_url=self.base_url_2,
                api_key=self.api_key_2 or self.api_key,
            )

    @classmethod
    def from_env(cls) -> LLMClient:
        """Create client from environment variables (loaded from .env)."""
        return cls(
            base_url=os.getenv("LLM_BASE_URL", "http://localhost:11434/v1"),
            api_key=os.getenv("LLM_API_KEY", "ollama"),
            base_url_2=os.getenv("LLM_BASE_URL_2"),
            api_key_2=os.getenv("LLM_API_KEY_2"),
            model=os.getenv("LLM_MODEL", "qwen/qwen3-1.7b"),
            temperature=float(os.getenv("LLM_TEMPERATURE", "0.0")),
            timeout=int(os.getenv("LLM_TIMEOUT", "900")),
            max_tokens=int(os.getenv("LLM_MAX_TOKENS", "0")),  # 0 = unlimited
        )

    @classmethod
    def all_from_env(cls) -> list[LLMClient]:
        """Create one client per model listed in LLM_MODELS (comma-separated).

        Falls back to a single client with LLM_MODEL if LLM_MODELS is not set.
        """
        models_str = os.getenv("LLM_MODELS", "")
        if not models_str:
            return [cls.from_env()]

        base = cls.from_env()
        clients = []
        for model in models_str.split(","):
            model = model.strip()
            if model:
                c = cls(
                    base_url=base.base_url,
                    api_key=base.api_key,
                    base_url_2=base.base_url_2,
                    api_key_2=base.api_key_2,
                    model=model,
                    temperature=base.temperature,
                    timeout=base.timeout,
                    max_tokens=base.max_tokens,
                )
                clients.append(c)
        return clients or [base]

    def _call_endpoint(self, client: OpenAI, messages: list[dict]) -> LLMResponse:
        """Call a single endpoint, return response or raise on failure."""
        t0 = time.time()
        kwargs = dict(
            model=self.model,
            messages=messages,
            temperature=self.temperature,
            timeout=self.timeout,
        )
        if self.max_tokens > 0:
            kwargs["max_tokens"] = self.max_tokens
        resp = client.chat.completions.create(**kwargs)
        latency = time.time() - t0
        choice = resp.choices[0]
        usage = resp.usage
        raw_content = choice.message.content or ""
        content = strip_think_blocks(raw_content)

        # If model spent all tokens thinking and produced no answer,
        # return the raw content so scoring can still extract something.
        if not content and raw_content:
            content = raw_content

        return LLMResponse(
            content=content,
            tokens_prompt=usage.prompt_tokens if usage else 0,
            tokens_completion=usage.completion_tokens if usage else 0,
            latency_s=latency,
            model=self.model,
        )

    def _call_with_failover(self, messages: list[dict]) -> LLMResponse:
        """Try primary endpoint, failover to secondary on error.

        Resilience: max_retries per endpoint, capped backoff,
        automatic failover to secondary endpoint.
        """
        clients = [self._client]
        if self._client_2:
            clients.append(self._client_2)

        last_error = ""
        for i, client in enumerate(clients):
            for attempt in range(self.max_retries + 1):
                t0 = time.time()
                try:
                    resp = self._call_endpoint(client, messages)
                    elapsed = time.time() - t0
                    log.debug(
                        "LLM call %s attempt=%d elapsed=%.1fs tokens=%d",
                        self.model, attempt, elapsed, resp.tokens_total,
                    )
                    return resp
                except Exception as e:
                    elapsed = time.time() - t0
                    last_error = str(e)
                    log.warning(
                        "LLM call %s attempt=%d failed after %.1fs: %s",
                        self.model, attempt, elapsed, last_error[:120],
                    )
                    if attempt < self.max_retries:
                        time.sleep(min(2 ** attempt, 8))
            # Primary exhausted, try secondary
            if i == 0 and len(clients) > 1:
                log.info("Failing over to secondary endpoint")
                continue

        return LLMResponse(
            content="", tokens_prompt=0, tokens_completion=0,
            latency_s=0.0, model=self.model, success=False, error=last_error,
        )

    def chat(
        self,
        user_message: str,
        system_message: str = "You are a helpful coding assistant.",
        *,
        context: str | None = None,
    ) -> LLMResponse:
        """Single-turn chat completion with dual-endpoint failover.

        If `context` is provided, it's prepended to the user message as
        grounding context (Synapseed output).
        """
        if context:
            user_message = (
                f"Use the following verified context to answer.\n"
                f"ONLY cite files and symbols that appear in this context.\n\n"
                f"--- CONTEXT ---\n{context}\n--- END CONTEXT ---\n\n"
                f"{user_message}"
            )

        messages = [
            {"role": "system", "content": system_message},
            {"role": "user", "content": user_message},
        ]

        return self._call_with_failover(messages)

    def multi_turn(
        self,
        turns: list[str],
        system_message: str = "You are a helpful coding assistant.",
        *,
        context: str | None = None,
    ) -> list[LLMResponse]:
        """Multi-turn conversation with failover. Returns one LLMResponse per turn."""
        messages = [{"role": "system", "content": system_message}]
        responses = []

        for i, user_msg in enumerate(turns):
            effective = user_msg
            if i == 0 and context:
                effective = (
                    f"Use the following verified context to answer.\n"
                    f"ONLY cite files and symbols that appear in this context.\n\n"
                    f"--- CONTEXT ---\n{context}\n--- END CONTEXT ---\n\n"
                    f"{user_msg}"
                )

            messages.append({"role": "user", "content": effective})
            resp = self._call_with_failover(messages)

            if resp.success:
                messages.append({"role": "assistant", "content": resp.content})
            responses.append(resp)

            if not resp.success:
                break

        return responses


def get_synapseed_context(
    query: str,
    repo_path: str,
    *,
    raw: bool = True,
    tier: str | None = None,
    timeout: int = 60,
) -> str | None:
    """Call `synapseed ask` as a subprocess and return the context string.

    Returns None on failure.
    """
    cmd = ["synapseed", "ask", query]
    if raw:
        cmd.append("--raw")

    env = {**os.environ, "RUST_LOG": "off"}
    if tier:
        env["SYNAPSEED_MODEL_TIER"] = tier

    try:
        result = subprocess.run(
            cmd,
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
        return None
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return None
