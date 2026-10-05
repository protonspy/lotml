"""Token counts for one text across several model families' tokenizers."""

from functools import cache

import tiktoken
from tokenizers import Tokenizer

TIKTOKEN = {"o200k": "o200k_base", "cl100k": "cl100k_base"}

# Repository and pinned commit for each open tokenizer; the unsloth mirrors stand in for
# the gated meta-llama and google repositories.
HUGGINGFACE = {
    "llama3": (
        "unsloth/Llama-3.2-1B-Instruct",
        "5a8abab4a5d6f164389b1079fb721cfab8d7126c",
    ),
    "qwen3": ("Qwen/Qwen3-8B", "b968826d9c46dd6066d109eabc6255188de91218"),
    "deepseek-v3": (
        "deepseek-ai/DeepSeek-V3",
        "e815299b0bcbac849fa540c768ef21845365c9eb",
    ),
    "gemma3": ("unsloth/gemma-3-1b-it", "5b11413a10db4e486ef16a20101fd028f8f2499c"),
    "mistral-nemo": (
        "mistralai/Mistral-Nemo-Instruct-2407",
        "04d8a90549d23fc6bd7f642064003592df51e9b3",
    ),
    "starcoder2": (
        "bigcode/starcoder2-15b",
        "46d44742909c03ac8cee08eb03fdebce02e193ec",
    ),
}

TOKENIZERS = {**TIKTOKEN, **HUGGINGFACE}


@cache
def load(name: str):
    """Return an encode function for `name` that never adds special tokens."""
    if name in TIKTOKEN:
        encoding = tiktoken.get_encoding(TIKTOKEN[name])
        return lambda text: encoding.encode(text, disallowed_special=())
    repo, revision = HUGGINGFACE[name]
    tokenizer = Tokenizer.from_pretrained(repo, revision=revision)
    return lambda text: tokenizer.encode(text, add_special_tokens=False).ids


def count(name: str, text: str) -> int:
    """Number of tokens `text` costs under tokenizer `name`, special tokens excluded."""
    return len(load(name)(text))
