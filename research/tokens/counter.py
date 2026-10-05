"""Token counts for one text across several model families' tokenizers."""

from functools import cache

import tiktoken
from tokenizers import Tokenizer

TIKTOKEN = {"o200k": "o200k_base", "cl100k": "cl100k_base"}

HUGGINGFACE = {
    "llama3": "unsloth/Llama-3.2-1B-Instruct",
    "qwen3": "Qwen/Qwen3-8B",
    "deepseek-v3": "deepseek-ai/DeepSeek-V3",
    "gemma3": "unsloth/gemma-3-1b-it",
    "mistral-nemo": "mistralai/Mistral-Nemo-Instruct-2407",
    "starcoder2": "bigcode/starcoder2-15b",
}

TOKENIZERS = {**TIKTOKEN, **HUGGINGFACE}


@cache
def load(name: str):
    """Return an encode function for `name` that never adds special tokens."""
    if name in TIKTOKEN:
        encoding = tiktoken.get_encoding(TIKTOKEN[name])
        return lambda text: encoding.encode(text, disallowed_special=())
    tokenizer = Tokenizer.from_pretrained(HUGGINGFACE[name])
    return lambda text: tokenizer.encode(text, add_special_tokens=False).ids


def count(name: str, text: str) -> int:
    """Number of tokens `text` costs under tokenizer `name`, special tokens excluded."""
    return len(load(name)(text))


def tokens(name: str, text: str) -> list[str]:
    """The pieces `text` splits into under tokenizer `name`, for inspection."""
    if name in TIKTOKEN:
        encoding = tiktoken.get_encoding(TIKTOKEN[name])
        return [encoding.decode([t]) for t in load(name)(text)]
    tokenizer = Tokenizer.from_pretrained(HUGGINGFACE[name])
    return [tokenizer.decode([t]) for t in load(name)(text)]
