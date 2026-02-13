"""OpenAI-compatible LLM client for local and remote models.

Supports Ollama, LM Studio, OpenRouter, and any OpenAI-compatible endpoint.
Configuration via .env or environment variables.
"""

from __future__ import annotations

import os
import subprocess
import time
from dataclasses import dataclass, field

from openai import OpenAI


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
    model: str = "qwen3-1.7b"
    temperature: float = 0.0
    timeout: int = 120
    max_retries: int = 2

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
            model=os.getenv("LLM_MODEL", "qwen3-1.7b"),
            temperature=float(os.getenv("LLM_TEMPERATURE", "0.0")),
            timeout=int(os.getenv("LLM_TIMEOUT", "120")),
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
                )
                clients.append(c)
        return clients or [base]

    def _call_endpoint(self, client: OpenAI, messages: list[dict]) -> LLMResponse:
        """Call a single endpoint, return response or raise on failure."""
        t0 = time.time()
        resp = client.chat.completions.create(
            model=self.model,
            messages=messages,
            temperature=self.temperature,
            timeout=self.timeout,
        )
        latency = time.time() - t0
        choice = resp.choices[0]
        usage = resp.usage
        return LLMResponse(
            content=choice.message.content or "",
            tokens_prompt=usage.prompt_tokens if usage else 0,
            tokens_completion=usage.completion_tokens if usage else 0,
            latency_s=latency,
            model=self.model,
        )

    def _call_with_failover(self, messages: list[dict]) -> LLMResponse:
        """Try primary endpoint, failover to secondary on error."""
        clients = [self._client]
        if self._client_2:
            clients.append(self._client_2)

        last_error = ""
        for i, client in enumerate(clients):
            for attempt in range(self.max_retries + 1):
                try:
                    return self._call_endpoint(client, messages)
                except Exception as e:
                    last_error = str(e)
                    if attempt < self.max_retries:
                        time.sleep(2**attempt)
            # Primary exhausted, try secondary
            if i == 0 and len(clients) > 1:
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
