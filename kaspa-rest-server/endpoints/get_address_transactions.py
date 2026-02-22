# encoding: utf-8
import time
from typing import List, Optional

from fastapi import Path, Query, HTTPException
from kaspa_script_address import to_script
from pydantic import BaseModel
from sqlalchemy import or_, exists
from sqlalchemy.future import select
from starlette.responses import Response

from constants import ADDRESS_EXAMPLE, REGEX_KASPA_ADDRESS, GENESIS_MS
from constants import USE_SCRIPT_FOR_ADDRESS
from dbsession import async_session
from endpoints import sql_db_only
from endpoints.get_transactions import (
    search_for_transactions,
    TxSearch,
    TxModel,
    PreviousOutpointLookupMode,
    AcceptanceMode,
)
from helper.utils import add_cache_control
from models.TransactionAcceptance import TransactionAcceptance
from models.TxAddrMapping import TxAddrMapping, TxScriptMapping
from server import app

DESC_RESOLVE_PARAM = (
    "Use this parameter if you want to fetch the TransactionInput previous outpoint details."
    " Light fetches only the adress and amount. Full fetches the whole TransactionOutput and "
    "adds it into each TxInput."
)


class TransactionsReceivedAndSpent(BaseModel):
    tx_received: str
    tx_spent: str | None
    # received_amount: int = 38240000000


class TransactionForAddressResponse(BaseModel):
    transactions: List[TransactionsReceivedAndSpent]


@app.get(
    "/addresses/{kaspaAddress}/full-transactions",
    response_model=List[TxModel],
    response_model_exclude_unset=True,
    tags=["Kaspa addresses"],
    openapi_extra={"strict_query_params": True},
)
@sql_db_only
async def get_full_transactions_for_address(
    response: Response,
    kaspa_address: str = Path(
        alias="kaspaAddress", description=f"Kaspa address as string e.g. {ADDRESS_EXAMPLE}", regex=REGEX_KASPA_ADDRESS
    ),
    limit: int = Query(description="The number of records to get", ge=1, le=500, default=50),
    offset: int = Query(description="The offset from which to get records", ge=0, default=0),
    fields: str = "",
    resolve_previous_outpoints: PreviousOutpointLookupMode = Query(default="no", description=DESC_RESOLVE_PARAM),
):
    """
    Get all transactions for a given address from database.
    And then get their related full transaction data
    """
    try:
        script = to_script(kaspa_address)
    except ValueError:
        raise HTTPException(status_code=400, detail=f"Invalid address: {kaspa_address}")

    async with async_session() as s:
        if USE_SCRIPT_FOR_ADDRESS:
            tx_within_limit_offset = await s.execute(
                select(TxScriptMapping.transaction_id, TxScriptMapping.block_time)
                .filter(TxScriptMapping.script_public_key == script)
                .limit(limit)
                .offset(offset)
                .order_by(TxScriptMapping.block_time.desc())
            )
        else:
            tx_within_limit_offset = await s.execute(
                select(TxAddrMapping.transaction_id, TxAddrMapping.block_time)
                .filter(TxAddrMapping.address == kaspa_address)
                .limit(limit)
                .offset(offset)
                .order_by(TxAddrMapping.block_time.desc())
            )

    tx_ids_in_page = []
    max_block_time = 0
    for tx_id, block_time in tx_within_limit_offset.all():
        tx_ids_in_page.append(tx_id)
        if block_time is not None and block_time > max_block_time:
            max_block_time = block_time

    if offset and max_block_time:
        delta_seconds = time.time() - int(max_block_time) / 1000
        if delta_seconds < 600:
            ttl = 8
        elif delta_seconds < 86400:  # 1 day
            ttl = 60
        else:
            ttl = 600
    else:
        ttl = 8
    response.headers["Cache-Control"] = f"public, max-age={ttl}"

    return await search_for_transactions(
        TxSearch(transactionIds=tx_ids_in_page, acceptingBlueScores=None), fields, resolve_previous_outpoints
    )


@app.get(
    "/addresses/{kaspaAddress}/full-transactions-page",
    response_model=List[TxModel],
    response_model_exclude_unset=True,
    tags=["Kaspa addresses"],
    openapi_extra={"strict_query_params": True},
)
@sql_db_only
async def get_full_transactions_for_address_page(
    response: Response,
    kaspa_address: str = Path(
        alias="kaspaAddress", description=f"Kaspa address as string e.g. {ADDRESS_EXAMPLE}", regex=REGEX_KASPA_ADDRESS
    ),
    limit: int = Query(
        description="The max number of records to get. "
        "For paging combine with using 'before/after' from oldest previous result. "
        "Use value of X-Next-Page-Before/-After as long as header is present to continue paging. "
        "The actual number of transactions returned for each page can be != limit.",
        ge=1,
        le=500,
        default=50,
    ),
    before: int = Query(
        description="Only include transactions with block time before this (epoch-millis)", ge=0, default=0
    ),
    after: int = Query(
        description="Only include transactions with block time after this (epoch-millis)", ge=0, default=0
    ),
    fields: str = "",
    resolve_previous_outpoints: PreviousOutpointLookupMode = Query(default="no", description=DESC_RESOLVE_PARAM),
    acceptance: Optional[AcceptanceMode] = Query(default=None),
):
    """
    Get all transactions for a given address from database.
    And then get their related full transaction data
    """
    try:
        script = to_script(kaspa_address)
    except ValueError:
        raise HTTPException(status_code=400, detail=f"Invalid address: {kaspa_address}")

    if USE_SCRIPT_FOR_ADDRESS:
        query = (
            select(TxScriptMapping.transaction_id, TxScriptMapping.block_time)
            .filter(TxScriptMapping.script_public_key == script)
            .limit(limit)
        )
    else:
        query = (
            select(TxAddrMapping.transaction_id, TxAddrMapping.block_time)
            .filter(TxAddrMapping.address == kaspa_address)
            .limit(limit)
        )

    response.headers["X-Page-Count"] = "0"
    if before != 0 and after != 0:
        raise HTTPException(status_code=400, detail="Only one of [before, after] can be present")
    elif before != 0:
        if before <= GENESIS_MS:
            return []
        if USE_SCRIPT_FOR_ADDRESS:
            query = query.filter(TxScriptMapping.block_time < before).order_by(TxScriptMapping.block_time.desc())
        else:
            query = query.filter(TxAddrMapping.block_time < before).order_by(TxAddrMapping.block_time.desc())
    elif after != 0:
        if after > int(time.time() * 1000) + 3600000:  # now + 1 hour
            return []
        if USE_SCRIPT_FOR_ADDRESS:
            query = query.filter(TxScriptMapping.block_time > after).order_by(TxScriptMapping.block_time.asc())
        else:
            query = query.filter(TxAddrMapping.block_time > after).order_by(TxAddrMapping.block_time.asc())
    else:
        if USE_SCRIPT_FOR_ADDRESS:
            query = query.order_by(TxScriptMapping.block_time.desc())
        else:
            query = query.order_by(TxAddrMapping.block_time.desc())

    if acceptance == AcceptanceMode.accepted:
        if USE_SCRIPT_FOR_ADDRESS:
            query = query.join(
                TransactionAcceptance, TxScriptMapping.transaction_id == TransactionAcceptance.transaction_id
            )
        else:
            query = query.join(
                TransactionAcceptance, TxAddrMapping.transaction_id == TransactionAcceptance.transaction_id
            )

    async with async_session() as s:
        tx_within_limit_before = await s.execute(query)

        tx_ids_and_block_times = [(x.transaction_id, x.block_time) for x in tx_within_limit_before.all()]
        if not tx_ids_and_block_times:
            return []

        tx_ids_and_block_times = sorted(tx_ids_and_block_times, key=lambda x: x[1], reverse=True)
        newest_block_time = tx_ids_and_block_times[0][1]
        oldest_block_time = tx_ids_and_block_times[-1][1]
        tx_ids = {tx_id for tx_id, block_time in tx_ids_and_block_times}
        if len(tx_ids_and_block_times) == limit:
            # To avoid gaps when transactions with the same block_time are at the intersection between pages.
            if USE_SCRIPT_FOR_ADDRESS:
                tx_with_same_block_time = await s.execute(
                    select(TxScriptMapping.transaction_id)
                    .filter(TxScriptMapping.script_public_key == script)
                    .filter(
                        or_(
                            TxScriptMapping.block_time == newest_block_time,
                            TxScriptMapping.block_time == oldest_block_time,
                        )
                    )
                )
            else:
                tx_with_same_block_time = await s.execute(
                    select(TxAddrMapping.transaction_id)
                    .filter(TxAddrMapping.address == kaspa_address)
                    .filter(
                        or_(
                            TxAddrMapping.block_time == newest_block_time, TxAddrMapping.block_time == oldest_block_time
                        )
                    )
                )
            tx_ids.update([x for x in tx_with_same_block_time.scalars().all()])

        # Check for more data before/after for pagination purposes
        if before or after:
            if USE_SCRIPT_FOR_ADDRESS:
                has_newer = await s.scalar(
                    select(
                        exists().where(
                            (TxScriptMapping.script_public_key == script)
                            & (TxScriptMapping.block_time > newest_block_time)
                        )
                    )
                )
            else:
                has_newer = await s.scalar(
                    select(
                        exists().where(
                            (TxAddrMapping.address == kaspa_address) & (TxAddrMapping.block_time > newest_block_time)
                        )
                    )
                )
        else:
            has_newer = False

        if not after or after >= GENESIS_MS:
            if USE_SCRIPT_FOR_ADDRESS:
                has_older = await s.scalar(
                    select(
                        exists().where(
                            (TxScriptMapping.script_public_key == script)
                            & (TxScriptMapping.block_time < oldest_block_time)
                        )
                    )
                )
            else:
                has_older = await s.scalar(
                    select(
                        exists().where(
                            (TxAddrMapping.address == kaspa_address) & (TxAddrMapping.block_time < oldest_block_time)
                        )
                    )
                )
        else:
            has_older = False

    if has_newer:
        response.headers["X-Next-Page-After"] = str(newest_block_time)
    if has_older:
        response.headers["X-Next-Page-Before"] = str(oldest_block_time)

    res = await search_for_transactions(
        TxSearch(transactionIds=list(tx_ids), acceptingBlueScores=None), fields, resolve_previous_outpoints, acceptance
    )
    response.headers["X-Page-Count"] = str(len(res))
    if before:
        add_cache_control(None, before, response)
    elif after and len(tx_ids) >= limit:
        max_block_time = max((r.get("block_time") for r in res))
        add_cache_control(None, max_block_time, response)
    return res
