# encoding: utf-8
import logging
import ssl
import time

import aiocache
import aiohttp
from aiocache import cached
try:
    import certifi
except Exception:  # pragma: no cover
    certifi = None

FLOOD_DETECTED = False
CACHE = None

_logger = logging.getLogger(__name__)

aiocache.logger.setLevel(logging.WARNING)


def _build_ssl_context():
    if certifi is None:
        return ssl.create_default_context()
    try:
        return ssl.create_default_context(cafile=certifi.where())
    except Exception as exc:  # pragma: no cover
        _logger.warning(f"Failed to initialize certifi CA bundle: {exc}")
        return ssl.create_default_context()


@cached(ttl=60)
async def get_kas_price():
    market_data = await get_kas_market_data()
    return market_data.get("current_price", {}).get("usd", 0)


@cached(ttl=60)
async def get_kas_market_data():
    global FLOOD_DETECTED
    global CACHE
    if not FLOOD_DETECTED or time.time() - FLOOD_DETECTED > 300:
        ssl_context = _build_ssl_context()
        connector = aiohttp.TCPConnector(ssl=ssl_context)
        async with aiohttp.ClientSession(connector=connector) as session:
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
            try:
                async with session.get("https://api.coingecko.com/api/v3/coins/kaspa", timeout=10) as resp:
                    if resp.status == 200:
                        FLOOD_DETECTED = False
                        CACHE = (await resp.json())["market_data"]
                        return CACHE
                    if resp.status == 429:
                        FLOOD_DETECTED = time.time()
                        if CACHE:
                            _logger.warning("Using cached value. 429 detected.")
                        _logger.warning("Rate limit exceeded.")
                    else:
                        _logger.error(f"Did not retrieve the market data. Status code {resp.status}")
            except Exception as e:
                _logger.warning(f"CoinGecko request failed: {e}")
                if CACHE:
                    _logger.warning("Using cached market data after CoinGecko failure.")

    return CACHE or {}
