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
    """OpenAI-compatible client with retry and token tracking.

    Usage::

        client = LLMClient.from_env()
        resp = client.chat("What is the Router struct?")
    """

    base_url: str = "http://localhost:11434/v1"
    api_key: str = "ollama"
    model: str = "qwen3:1.7b"
    temperature: float = 0.0
    timeout: int = 120
    max_retries: int = 2

    _client: OpenAI = field(init=False, repr=False)

    def __post_init__(self):
        self._client = OpenAI(base_url=self.base_url, api_key=self.api_key)

    @classmethod
    def from_env(cls) -> LLMClient:
        """Create client from environment variables (loaded from .env)."""
        return cls(
            base_url=os.getenv("LLM_BASE_URL", "http://localhost:11434/v1"),
            api_key=os.getenv("LLM_API_KEY", "ollama"),
            model=os.getenv("LLM_MODEL", "qwen3:1.7b"),
            temperature=float(os.getenv("LLM_TEMPERATURE", "0.0")),
            timeout=int(os.getenv("LLM_TIMEOUT", "120")),
        )

    def chat(
        self,
        user_message: str,
        system_message: str = "You are a helpful coding assistant.",
        *,
        context: str | None = None,
    ) -> LLMResponse:
        """Single-turn chat completion.

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

        for attempt in range(self.max_retries + 1):
            try:
                t0 = time.time()
                resp = self._client.chat.completions.create(
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
            except Exception as e:
                if attempt == self.max_retries:
                    return LLMResponse(
                        content="",
                        tokens_prompt=0,
                        tokens_completion=0,
                        latency_s=0.0,
                        model=self.model,
                        success=False,
                        error=str(e),
                    )
                time.sleep(2**attempt)

        # Unreachable, but keeps mypy happy
        return LLMResponse(
            content="", tokens_prompt=0, tokens_completion=0,
            latency_s=0.0, model=self.model, success=False, error="exhausted retries",
        )

    def multi_turn(
        self,
        turns: list[str],
        system_message: str = "You are a helpful coding assistant.",
        *,
        context: str | None = None,
    ) -> list[LLMResponse]:
        """Multi-turn conversation. Returns one LLMResponse per turn."""
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

            try:
                t0 = time.time()
                resp = self._client.chat.completions.create(
                    model=self.model,
                    messages=messages,
                    temperature=self.temperature,
                    timeout=self.timeout,
                )
                latency = time.time() - t0
                choice = resp.choices[0]
                usage = resp.usage
                content = choice.message.content or ""

                messages.append({"role": "assistant", "content": content})
                responses.append(LLMResponse(
                    content=content,
                    tokens_prompt=usage.prompt_tokens if usage else 0,
                    tokens_completion=usage.completion_tokens if usage else 0,
                    latency_s=latency,
                    model=self.model,
                ))
            except Exception as e:
                responses.append(LLMResponse(
                    content="", tokens_prompt=0, tokens_completion=0,
                    latency_s=0.0, model=self.model, success=False, error=str(e),
                ))
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
