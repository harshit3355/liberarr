from openai import OpenAI

from liberarr.shared.config import Settings


def get_client(settings: Settings) -> OpenAI:
    return OpenAI(base_url=settings.ai_base_url, api_key=settings.ai_api_key)
