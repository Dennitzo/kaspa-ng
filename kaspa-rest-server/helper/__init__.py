# encoding: utf-8
import logging
import time

import aiocache
import aiohttp
from aiocache import cached

FLOOD_DETECTED = False
CACHE = None

_logger = logging.getLogger(__name__)

aiocache.logger.setLevel(logging.WARNING)


@cached(ttl=60)
async def get_kas_price():
    market_data = await get_kas_market_data()
    return market_data.get("current_price", {}).get("usd", 0)


@cached(ttl=60)
async def get_kas_market_data():
    global FLOOD_DETECTED
    global CACHE
    if not FLOOD_DETECTED or time.time() - FLOOD_DETECTED > 300:
        async with aiohttp.ClientSession() as session:
            try:
                _logger.debug("Querying CoinGecko mirror")
                async with session.get("https://price.kaspa.ws/cg.json", timeout=10) as resp:
                    if resp.status == 200:
                        CACHE = (await resp.json())["market_data"]
                        FLOOD_DETECTED = False
                        return CACHE
            except Exception:
                pass  # Ignore and fall back
            _logger.info("Mirror failed, querying CoinGecko")
            async with session.get("https://api.coingecko.com/api/v3/coins/kaspa", timeout=10) as resp:
                if resp.status == 200:
                    FLOOD_DETECTED = False
                    CACHE = (await resp.json())["market_data"]
                    return CACHE
                elif resp.status == 429:
                    FLOOD_DETECTED = time.time()
                    if CACHE:
                        _logger.warning("Using cached value. 429 detected.")
                    _logger.warning("Rate limit exceeded.")
                else:
                    _logger.error(f"Did not retrieve the market data. Status code {resp.status}")

    return CACHE
