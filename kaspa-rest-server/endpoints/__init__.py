# encoding: utf-8
import os
import time
from fastapi import HTTPException
from functools import wraps
from sqlalchemy import text

from constants import NETWORK_TYPE
from dbsession import async_session

_SCHEMA_READY = False
_SCHEMA_LAST_CHECK = 0.0
_SCHEMA_CHECK_INTERVAL = 5.0


async def _schema_ready() -> bool:
    global _SCHEMA_READY, _SCHEMA_LAST_CHECK
    now = time.monotonic()
    if now - _SCHEMA_LAST_CHECK < _SCHEMA_CHECK_INTERVAL:
        return _SCHEMA_READY

    _SCHEMA_LAST_CHECK = now
    try:
        async with async_session() as s:
            result = await s.execute(
                text(
                    "SELECT 1 FROM information_schema.tables "
                    "WHERE table_schema = 'public' AND table_name = 'vars' "
                    "LIMIT 1"
                )
            )
            _SCHEMA_READY = result.scalar() is not None
    except Exception:
        _SCHEMA_READY = False

    return _SCHEMA_READY


def filter_fields(response_dict, fields):
    if fields:
        return {k: v for k, v in response_dict.items() if k in fields}
    else:
        return response_dict


def sql_db_only(func):
    @wraps(func)
    async def wrapper(*args, **kwargs):
        if not os.getenv("SQL_URI"):
            raise HTTPException(
                status_code=503, detail="Endpoint not available. This endpoint needs a configured SQL database."
            )
        if not await _schema_ready():
            raise HTTPException(
                status_code=503, detail="Indexer syncing. Database schema not ready yet."
            )
        return await func(*args, **kwargs)

    return wrapper


def mainnet_only(func):
    @wraps(func)
    async def wrapper(*args, **kwargs):
        if NETWORK_TYPE != "mainnet":
            raise HTTPException(
                status_code=503, detail="Endpoint not available. This endpoint is only available in mainnet."
            )
        return await func(*args, **kwargs)

    return wrapper
