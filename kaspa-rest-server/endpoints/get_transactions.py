# encoding: utf-8
import asyncio
import json
import logging
from collections import defaultdict
from enum import Enum
from typing import List, Optional

from kaspa_script_address import to_address
from fastapi import Path, HTTPException, Query
from pydantic import BaseModel, Field
from sqlalchemy import exists, text
from sqlalchemy.future import select
from starlette.responses import Response

from constants import TX_SEARCH_ID_LIMIT, TX_SEARCH_BS_LIMIT, PREV_OUT_RESOLVED, ADDRESS_PREFIX
from dbsession import async_session, async_session_blocks
from endpoints import filter_fields, sql_db_only
from endpoints.get_blocks import get_block_from_kaspad
from helper.PublicKeyType import get_public_key_type
from helper.utils import add_cache_control
from models.Block import Block
from models.BlockTransaction import BlockTransaction
from models.Subnetwork import Subnetwork
from models.Transaction import Transaction, TransactionOutput, TransactionInput
from models.TransactionAcceptance import TransactionAcceptance
from server import app

_logger = logging.getLogger(__name__)
_legacy_tx_io_tables: bool | None = None

DESC_RESOLVE_PARAM = (
    "Use this parameter if you want to fetch the TransactionInput previous outpoint details."
    " Light fetches only the address and amount. Full fetches the whole TransactionOutput and "
    "adds it into each TxInput."
)


class TxOutput(BaseModel):
    transaction_id: str
    index: int
    amount: int
    script_public_key: str | None
    script_public_key_address: str | None
    script_public_key_type: str | None
    accepting_block_hash: str | None

    class Config:
        from_attributes = True


class TxInput(BaseModel):
    transaction_id: str
    index: int
    previous_outpoint_hash: str
    previous_outpoint_index: str
    previous_outpoint_resolved: TxOutput | None
    previous_outpoint_address: str | None
    previous_outpoint_amount: int | None
    signature_script: str | None
    sig_op_count: str | None

    class Config:
        from_attributes = True


class TxModel(BaseModel):
    subnetwork_id: str | None
    transaction_id: str | None
    hash: str | None
    mass: str | None
    payload: str | None
    block_hash: List[str] | None
    block_time: int | None
    is_accepted: bool | None
    accepting_block_hash: str | None
    accepting_block_blue_score: int | None
    accepting_block_time: int | None
    inputs: List[TxInput] | None
    outputs: List[TxOutput] | None

    class Config:
        from_attributes = True


class TxSearchAcceptingBlueScores(BaseModel):
    gte: int
    lt: int


class TxSearch(BaseModel):
    transactionIds: List[str] | None
    acceptingBlueScores: TxSearchAcceptingBlueScores | None


class TxAcceptanceRequest(BaseModel):
    transactionIds: list[str] = Field(
        examples=[
            "b9382bdee4aa364acf73eda93914eaae61d0e78334d1b8a637ab89ef5e224e41",
            "1e098b3830c994beb28768f7924a38286cec16e85e9757e0dc3574b85f624c34",
            "000ad5138a603aadc25cfcca6b6605d5ff47d8c7be594c9cdd199afa6dc76ac6",
        ]
    )


class TxAcceptanceResponse(BaseModel):
    transactionId: str = "b9382bdee4aa364acf73eda93914eaae61d0e78334d1b8a637ab89ef5e224e41"
    accepted: bool
    acceptingBlockHash: str | None
    acceptingBlueScore: int | None
    acceptingTimestamp: int | None


class PreviousOutpointLookupMode(str, Enum):
    no = "no"
    light = "light"
    full = "full"


class AcceptanceMode(str, Enum):
    accepted = "accepted"
    rejected = "rejected"


@app.get(
    "/transactions/{transactionId}",
    response_model=TxModel,
    tags=["Kaspa transactions"],
    response_model_exclude_unset=True,
)
@sql_db_only
async def get_transaction(
    response: Response,
    transactionId: str = Path(pattern="[a-f0-9]{64}"),
    blockHash: str = Query(None, description="Specify a containing block (if known) for faster lookup"),
    inputs: bool = True,
    outputs: bool = True,
    resolve_previous_outpoints: PreviousOutpointLookupMode = Query(
        default=PreviousOutpointLookupMode.no, description=DESC_RESOLVE_PARAM
    ),
):
    """
    Get details for a given transaction id
    """
    res_outpoints = resolve_previous_outpoints
    async with async_session_blocks() as session_blocks:
        async with async_session() as session:
            transaction = None
            if blockHash:
                block_hashes = [blockHash]
            else:
                block_hashes = await session_blocks.execute(
                    select(BlockTransaction.block_hash).filter(BlockTransaction.transaction_id == transactionId)
                )
                block_hashes = block_hashes.scalars().all()
            if block_hashes:
                transaction = await get_transaction_from_kaspad(block_hashes, transactionId, inputs, outputs)
                if transaction and inputs and res_outpoints == "light" and PREV_OUT_RESOLVED:
                    tx_inputs = await get_tx_inputs_from_db(None, res_outpoints, [transactionId])
                    if transactionId in tx_inputs:
                        transaction["inputs"] = tx_inputs[transactionId]

            if not transaction:
                tx = await session.execute(
                    select(Transaction, Subnetwork)
                    .join(Subnetwork, Transaction.subnetwork_id == Subnetwork.id)
                    .filter(Transaction.transaction_id == transactionId)
                )
                tx = tx.first()

                if tx:
                    logging.debug(f"Found transaction {transactionId} in database")
                    transaction = {
                        "subnetwork_id": tx.Subnetwork.subnetwork_id,
                        "transaction_id": tx.Transaction.transaction_id,
                        "hash": tx.Transaction.hash,
                        "mass": tx.Transaction.mass,
                        "payload": tx.Transaction.payload,
                        "block_hash": block_hashes,
                        "block_time": tx.Transaction.block_time,
                    }

                    if inputs and (res_outpoints != "light" or PREV_OUT_RESOLVED) and res_outpoints != "full":
                        tx_inputs = await get_tx_inputs_from_db(None, res_outpoints, [transactionId])
                        transaction["inputs"] = tx_inputs.get(transactionId) or None

                    if outputs:
                        tx_outputs = await get_tx_outputs_from_db(None, [transactionId])
                        transaction["outputs"] = tx_outputs.get(transactionId) or None

            if transaction:
                if inputs and res_outpoints == "light" and not PREV_OUT_RESOLVED or res_outpoints == "full":
                    tx_inputs = await get_tx_inputs_from_db(None, res_outpoints, [transactionId])
                    if transactionId in tx_inputs:
                        transaction["inputs"] = tx_inputs[transactionId]

                accepted_transaction_id, accepting_block_hash = (
                    await session.execute(
                        select(
                            TransactionAcceptance.transaction_id,
                            TransactionAcceptance.block_hash,
                        ).filter(TransactionAcceptance.transaction_id == transactionId)
                    )
                ).one_or_none() or (None, None)
                transaction["is_accepted"] = accepted_transaction_id is not None

                if accepting_block_hash:
                    accepting_block_blue_score, accepting_block_time = (
                        await session_blocks.execute(
                            select(
                                Block.blue_score,
                                Block.timestamp,
                            ).filter(Block.hash == accepting_block_hash)
                        )
                    ).one_or_none() or (None, None)
                    transaction["accepting_block_hash"] = accepting_block_hash
                    transaction["accepting_block_blue_score"] = accepting_block_blue_score
                    transaction["accepting_block_time"] = accepting_block_time
                    if not accepting_block_blue_score:
                        accepting_block = await get_block_from_kaspad(accepting_block_hash, False, False)
                        accepting_block_header = accepting_block.get("header") if accepting_block else None
                        if accepting_block_header:
                            transaction["accepting_block_blue_score"] = accepting_block_header.get("blueScore")
                            transaction["accepting_block_time"] = accepting_block_header.get("timestamp")

    if transaction:
        add_cache_control(transaction.get("accepting_block_blue_score"), transaction.get("block_time"), response)
        return transaction
    else:
        raise HTTPException(
            status_code=404, detail="Transaction not found", headers={"Cache-Control": "public, max-age=3"}
        )


@app.post(
    "/transactions/search", response_model=List[TxModel], tags=["Kaspa transactions"], response_model_exclude_unset=True
)
@sql_db_only
async def search_for_transactions(
    txSearch: TxSearch,
    fields: str = Query(default=""),
    resolve_previous_outpoints: PreviousOutpointLookupMode = Query(
        default=PreviousOutpointLookupMode.no, description=DESC_RESOLVE_PARAM
    ),
    acceptance: Optional[AcceptanceMode] = Query(
        default=None, description="Only used when searching using transactionIds"
    ),
):
    """
    Search for transactions by transaction_ids or blue_score
    """
    if not txSearch.transactionIds and not txSearch.acceptingBlueScores:
        return []

    if txSearch.transactionIds and len(txSearch.transactionIds) > TX_SEARCH_ID_LIMIT:
        raise HTTPException(422, f"Too many transaction ids. Max {TX_SEARCH_ID_LIMIT}")

    if txSearch.transactionIds and txSearch.acceptingBlueScores:
        raise HTTPException(422, "Only one of transactionIds and acceptingBlueScores must be non-null")

    if (
        txSearch.acceptingBlueScores
        and txSearch.acceptingBlueScores.lt - txSearch.acceptingBlueScores.gte > TX_SEARCH_BS_LIMIT
    ):
        raise HTTPException(400, f"Diff between acceptingBlueScores.gte and lt must be <= {TX_SEARCH_BS_LIMIT}")

    transaction_ids = set(txSearch.transactionIds or [])
    accepting_blue_score_gte = txSearch.acceptingBlueScores.gte if txSearch.acceptingBlueScores else None
    accepting_blue_score_lt = txSearch.acceptingBlueScores.lt if txSearch.acceptingBlueScores else None

    fields = fields.split(",") if fields else []

    async with async_session() as session:
        async with async_session_blocks() as session_blocks:
            tx_query = (
                select(
                    Transaction,
                    Subnetwork,
                    TransactionAcceptance.transaction_id.label("accepted_transaction_id"),
                    TransactionAcceptance.block_hash.label("accepting_block_hash"),
                )
                .join(Subnetwork, Transaction.subnetwork_id == Subnetwork.id)
                .outerjoin(TransactionAcceptance, Transaction.transaction_id == TransactionAcceptance.transaction_id)
                .order_by(Transaction.block_time.desc())
            )

            if accepting_blue_score_gte:
                tx_acceptances = await session_blocks.execute(
                    select(
                        Block.hash.label("accepting_block_hash"),
                        Block.blue_score.label("accepting_block_blue_score"),
                        Block.timestamp.label("accepting_block_time"),
                    )
                    .filter(exists().where(TransactionAcceptance.block_hash == Block.hash))  # Only chain blocks
                    .filter(Block.blue_score >= accepting_blue_score_gte)
                    .filter(Block.blue_score < accepting_blue_score_lt)
                )
                tx_acceptances = {row.accepting_block_hash: row for row in tx_acceptances.all()}
                if not tx_acceptances:
                    return []
                tx_query = tx_query.filter(TransactionAcceptance.block_hash.in_(tx_acceptances.keys()))
                tx_list = (await session.execute(tx_query)).all()
                transaction_ids = [row.Transaction.transaction_id for row in tx_list]
            else:
                tx_query = tx_query.filter(Transaction.transaction_id.in_(transaction_ids))
                if acceptance == AcceptanceMode.accepted:
                    tx_query = tx_query.filter(TransactionAcceptance.transaction_id.is_not(None))
                elif acceptance == AcceptanceMode.rejected:
                    tx_query = tx_query.filter(TransactionAcceptance.transaction_id.is_(None))
                tx_list = (await session.execute(tx_query)).all()
                if not tx_list:
                    return []
                accepting_block_hashes = [
                    row.accepting_block_hash for row in tx_list if row.accepting_block_hash is not None
                ]
                tx_acceptances = await session_blocks.execute(
                    select(
                        Block.hash.label("accepting_block_hash"),
                        Block.blue_score.label("accepting_block_blue_score"),
                        Block.timestamp.label("accepting_block_time"),
                    ).filter(Block.hash.in_(accepting_block_hashes))
                )
                tx_acceptances = {row.accepting_block_hash: row for row in tx_acceptances.all()}

    async_tasks = [
        get_tx_blocks_from_db(fields, transaction_ids),
        get_tx_inputs_from_db(fields, resolve_previous_outpoints, transaction_ids),
        get_tx_outputs_from_db(fields, transaction_ids),
    ]
    tx_blocks, tx_inputs, tx_outputs = await asyncio.gather(*async_tasks)

    block_cache = {}
    results = []
    for tx in tx_list:
        accepting_block_blue_score = None
        accepting_block_time = None
        accepting_block = tx_acceptances.get(tx.accepting_block_hash)
        if accepting_block:
            accepting_block_blue_score = accepting_block.accepting_block_blue_score
            accepting_block_time = accepting_block.accepting_block_time
        else:
            if tx.accepting_block_hash:
                if tx.accepting_block_hash not in block_cache:
                    block_cache[tx.accepting_block_hash] = await get_block_from_kaspad(
                        tx.accepting_block_hash, False, False
                    )
                accepting_block = block_cache[tx.accepting_block_hash]
                if accepting_block and accepting_block["header"]:
                    accepting_block_blue_score = accepting_block["header"]["blueScore"]
                    accepting_block_time = accepting_block["header"]["timestamp"]

        result = filter_fields(
            {
                "subnetwork_id": tx.Subnetwork.subnetwork_id,
                "transaction_id": tx.Transaction.transaction_id,
                "hash": tx.Transaction.hash,
                "mass": tx.Transaction.mass,
                "payload": tx.Transaction.payload,
                "block_hash": tx_blocks.get(tx.Transaction.transaction_id),
                "block_time": tx.Transaction.block_time,
                "is_accepted": True if tx.accepted_transaction_id else False,
                "accepting_block_hash": tx.accepting_block_hash,
                "accepting_block_blue_score": accepting_block_blue_score,
                "accepting_block_time": accepting_block_time,
                "outputs": tx_outputs.get(tx.Transaction.transaction_id),
                "inputs": tx_inputs.get(tx.Transaction.transaction_id),
            },
            fields,
        )
        results.append(result)
    return results


@app.post(
    "/transactions/acceptance",
    response_model=List[TxAcceptanceResponse],
    response_model_exclude_unset=True,
    tags=["Kaspa transactions"],
    openapi_extra={"strict_query_params": True},
)
@sql_db_only
async def get_transaction_acceptance(tx_acceptance_request: TxAcceptanceRequest):
    """
    Given a list of transaction_ids, return whether each one is accepted and the accepting blue score and timestamp.
    """
    transaction_ids = tx_acceptance_request.transactionIds
    if len(transaction_ids) > TX_SEARCH_ID_LIMIT:
        raise HTTPException(422, f"Too many transaction ids. Max {TX_SEARCH_ID_LIMIT}")

    async with async_session() as s:
        result = await s.execute(
            select(TransactionAcceptance.transaction_id, TransactionAcceptance.block_hash).where(
                TransactionAcceptance.transaction_id.in_(set(transaction_ids))
            )
        )
        transaction_id_to_block_hash = {tx_id: block_hash for tx_id, block_hash in result}

    async with async_session_blocks() as s:
        result = await s.execute(
            select(Block.hash, Block.blue_score, Block.timestamp).where(
                Block.hash.in_(set(transaction_id_to_block_hash.values()))
            )
        )
        block_hash_to_info = {block_hash: (blue_score, timestamp) for block_hash, blue_score, timestamp in result}

    responses = []
    for tx_id in transaction_ids:
        block_hash = transaction_id_to_block_hash.get(tx_id)
        blue_score, timestamp = block_hash_to_info.get(block_hash, (None, None))
        responses.append(
            TxAcceptanceResponse(
                transactionId=tx_id,
                accepted=block_hash is not None,
                acceptingBlockHash=block_hash,
                acceptingBlueScore=blue_score,
                acceptingTimestamp=timestamp,
            )
        )
    return responses


async def get_tx_blocks_from_db(fields, transaction_ids):
    tx_blocks_dict = defaultdict(list)
    if fields and "block_hash" not in fields:
        return tx_blocks_dict

    async with async_session_blocks() as session_blocks:
        tx_blocks = await session_blocks.execute(
            select(BlockTransaction).filter(BlockTransaction.transaction_id.in_(transaction_ids))
        )
        for row in tx_blocks.scalars().all():
            tx_blocks_dict[row.transaction_id].append(row.block_hash)
        return tx_blocks_dict


async def _has_legacy_tx_io_tables(session) -> bool:
    global _legacy_tx_io_tables
    if _legacy_tx_io_tables is not None:
        return _legacy_tx_io_tables

    result = await session.execute(
        text(
            """
            SELECT
                EXISTS (
                    SELECT 1
                    FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = 'transactions_inputs'
                )
                AND EXISTS (
                    SELECT 1
                    FROM information_schema.tables
                    WHERE table_schema = 'public' AND table_name = 'transactions_outputs'
                )
            """
        )
    )
    _legacy_tx_io_tables = bool(result.scalar())
    return _legacy_tx_io_tables


def _to_tx_id_bytes(transaction_ids) -> list[bytes]:
    tx_id_bytes: list[bytes] = []
    seen = set()
    for tx_id in transaction_ids:
        if isinstance(tx_id, (bytes, bytearray)):
            candidate = bytes(tx_id)
        elif isinstance(tx_id, str):
            try:
                candidate = bytes.fromhex(tx_id)
            except ValueError:
                continue
        else:
            continue
        if candidate in seen:
            continue
        seen.add(candidate)
        tx_id_bytes.append(candidate)
    return tx_id_bytes


def _normalize_hex(value):
    if value is None:
        return None
    if isinstance(value, (bytes, bytearray, memoryview)):
        return bytes(value).hex()
    if isinstance(value, str):
        return value[2:] if value.startswith("\\x") else value
    return value


def _read_value(payload: dict, *keys):
    for key in keys:
        if key in payload:
            return payload[key]
    return None


def _ensure_json_list(value):
    if value is None:
        return []
    if isinstance(value, str):
        try:
            value = json.loads(value)
        except Exception:
            return []
    return value if isinstance(value, list) else []


def _safe_to_address(script_hex: str | None) -> str | None:
    if not script_hex:
        return None
    try:
        return to_address(ADDRESS_PREFIX, script_hex)
    except Exception:
        return None


def _map_output_dict(transaction_id: str, output: dict) -> dict:
    script_public_key = _normalize_hex(_read_value(output, "script_public_key", "scriptPublicKey"))
    script_public_key_address = _read_value(output, "script_public_key_address", "scriptPublicKeyAddress")
    return {
        "transaction_id": transaction_id,
        "index": int(_read_value(output, "index") or 0),
        "amount": int(_read_value(output, "amount") or 0),
        "script_public_key": script_public_key,
        "script_public_key_address": script_public_key_address or _safe_to_address(script_public_key),
        "script_public_key_type": get_public_key_type(script_public_key) if script_public_key else None,
        "accepting_block_hash": None,
    }


async def _load_outputs_from_transactions_array(session, transaction_ids):
    tx_outputs_dict = defaultdict(list)
    tx_id_bytes = _to_tx_id_bytes(transaction_ids)
    if not tx_id_bytes:
        return tx_outputs_dict

    rows = await session.execute(
        text(
            """
            SELECT
                encode(t.transaction_id, 'hex') AS transaction_id,
                COALESCE(to_jsonb(t.outputs), '[]'::jsonb) AS outputs
            FROM transactions t
            WHERE t.transaction_id = ANY(:transaction_ids)
            """
        ),
        {"transaction_ids": tx_id_bytes},
    )

    for row in rows.mappings():
        tx_id = row["transaction_id"]
        outputs = _ensure_json_list(row["outputs"])
        tx_outputs_dict[tx_id] = [_map_output_dict(tx_id, output) for output in outputs]

    return tx_outputs_dict


async def _load_output_index_from_transactions_array(session, transaction_ids):
    output_map = {}
    outputs_by_tx = await _load_outputs_from_transactions_array(session, transaction_ids)
    for tx_id, outputs in outputs_by_tx.items():
        for output in outputs:
            output_map[(tx_id, int(output["index"]))] = output
    return output_map


async def get_tx_inputs_from_db(fields, resolve_previous_outpoints, transaction_ids):
    tx_inputs_dict = defaultdict(list)
    if fields and "inputs" not in fields:
        return tx_inputs_dict

    async with async_session() as session:
        if await _has_legacy_tx_io_tables(session):
            if resolve_previous_outpoints == "light" and not PREV_OUT_RESOLVED or resolve_previous_outpoints == "full":
                tx_inputs = await session.execute(
                    select(TransactionInput, TransactionOutput)
                    .outerjoin(
                        TransactionOutput,
                        (TransactionOutput.transaction_id == TransactionInput.previous_outpoint_hash)
                        & (TransactionOutput.index == TransactionInput.previous_outpoint_index),
                    )
                    .filter(TransactionInput.transaction_id.in_(transaction_ids))
                    .order_by(TransactionInput.transaction_id, TransactionInput.index)
                )
                for tx_input, tx_prev_output in tx_inputs.all():
                    if tx_prev_output:
                        tx_input.previous_outpoint_script = tx_prev_output.script_public_key
                        tx_input.previous_outpoint_amount = tx_prev_output.amount
                        if resolve_previous_outpoints == "full":
                            tx_input.previous_outpoint_resolved = tx_prev_output
                    else:
                        tx_input.previous_outpoint_script = None
                        tx_input.previous_outpoint_amount = None
                        if resolve_previous_outpoints == "full":
                            tx_input.previous_outpoint_resolved = None
                    tx_inputs_dict[tx_input.transaction_id].append(tx_input)
            else:
                tx_inputs = await session.execute(
                    select(TransactionInput)
                    .filter(TransactionInput.transaction_id.in_(transaction_ids))
                    .order_by(TransactionInput.transaction_id, TransactionInput.index)
                )
                for tx_input in tx_inputs.scalars().all():
                    if resolve_previous_outpoints == "no" and PREV_OUT_RESOLVED:
                        tx_input.previous_outpoint_script = None
                        tx_input.previous_outpoint_amount = None
                    tx_inputs_dict[tx_input.transaction_id].append(tx_input)
            return tx_inputs_dict

        tx_id_bytes = _to_tx_id_bytes(transaction_ids)
        if not tx_id_bytes:
            return tx_inputs_dict

        rows = await session.execute(
            text(
                """
                SELECT
                    encode(t.transaction_id, 'hex') AS transaction_id,
                    COALESCE(to_jsonb(t.inputs), '[]'::jsonb) AS inputs
                FROM transactions t
                WHERE t.transaction_id = ANY(:transaction_ids)
                """
            ),
            {"transaction_ids": tx_id_bytes},
        )

        input_rows: list[dict] = []
        for row in rows.mappings():
            tx_id = row["transaction_id"]
            inputs = _ensure_json_list(row["inputs"])
            for tx_input in inputs:
                previous_outpoint_hash = _normalize_hex(_read_value(tx_input, "previous_outpoint_hash"))
                previous_outpoint_index = int(_read_value(tx_input, "previous_outpoint_index") or 0)
                previous_outpoint_script = _normalize_hex(_read_value(tx_input, "previous_outpoint_script"))
                previous_outpoint_amount = _read_value(tx_input, "previous_outpoint_amount")
                mapped = {
                    "transaction_id": tx_id,
                    "index": int(_read_value(tx_input, "index") or 0),
                    "previous_outpoint_hash": previous_outpoint_hash,
                    "previous_outpoint_index": str(previous_outpoint_index),
                    "previous_outpoint_script": previous_outpoint_script,
                    "previous_outpoint_amount": int(previous_outpoint_amount) if previous_outpoint_amount is not None else None,
                    "previous_outpoint_address": _safe_to_address(previous_outpoint_script),
                    "signature_script": _normalize_hex(_read_value(tx_input, "signature_script")),
                    "sig_op_count": str(_read_value(tx_input, "sig_op_count") or 0),
                    "previous_outpoint_resolved": None,
                }
                input_rows.append(mapped)

        if resolve_previous_outpoints == "no":
            if PREV_OUT_RESOLVED:
                for tx_input in input_rows:
                    tx_input["previous_outpoint_script"] = None
                    tx_input["previous_outpoint_amount"] = None
                    tx_input["previous_outpoint_address"] = None
        else:
            should_resolve = resolve_previous_outpoints == "full" or not PREV_OUT_RESOLVED
            if should_resolve:
                previous_outpoint_ids = {
                    tx_input["previous_outpoint_hash"]
                    for tx_input in input_rows
                    if tx_input["previous_outpoint_hash"]
                }
                output_map = await _load_output_index_from_transactions_array(session, previous_outpoint_ids)
                for tx_input in input_rows:
                    key = (tx_input["previous_outpoint_hash"], int(tx_input["previous_outpoint_index"]))
                    previous_output = output_map.get(key)
                    if previous_output:
                        if tx_input["previous_outpoint_script"] is None:
                            tx_input["previous_outpoint_script"] = previous_output["script_public_key"]
                        if tx_input["previous_outpoint_amount"] is None:
                            tx_input["previous_outpoint_amount"] = previous_output["amount"]
                        tx_input["previous_outpoint_address"] = _safe_to_address(tx_input["previous_outpoint_script"])
                        if resolve_previous_outpoints == "full":
                            tx_input["previous_outpoint_resolved"] = previous_output
            elif resolve_previous_outpoints == "full":
                for tx_input in input_rows:
                    tx_input["previous_outpoint_resolved"] = None

        input_rows.sort(key=lambda x: (x["transaction_id"], x["index"]))
        for tx_input in input_rows:
            tx_inputs_dict[tx_input["transaction_id"]].append(tx_input)
        return tx_inputs_dict


async def get_tx_outputs_from_db(fields, transaction_ids):
    tx_outputs_dict = defaultdict(list)
    if fields and "outputs" not in fields:
        return tx_outputs_dict

    async with async_session() as session:
        if await _has_legacy_tx_io_tables(session):
            tx_outputs = await session.execute(
                select(TransactionOutput)
                .filter(TransactionOutput.transaction_id.in_(transaction_ids))
                .order_by(TransactionOutput.transaction_id, TransactionOutput.index)
            )
            for tx_output in tx_outputs.scalars().all():
                tx_outputs_dict[tx_output.transaction_id].append(tx_output)
            return tx_outputs_dict

        tx_outputs_dict = await _load_outputs_from_transactions_array(session, transaction_ids)
        return tx_outputs_dict


async def get_transaction_from_kaspad(block_hashes, transaction_id, include_inputs, include_outputs):
    block = await get_block_from_kaspad(block_hashes[0], True, False)
    return map_transaction_from_kaspad(block, transaction_id, block_hashes, include_inputs, include_outputs)


def map_transaction_from_kaspad(block, transaction_id, block_hashes, include_inputs, include_outputs):
    if block and "transactions" in block:
        for tx in block["transactions"]:
            if tx["verboseData"]["transactionId"] == transaction_id:
                return {
                    "subnetwork_id": tx["subnetworkId"],
                    "transaction_id": tx["verboseData"]["transactionId"],
                    "hash": tx["verboseData"]["hash"],
                    "mass": tx["verboseData"]["computeMass"]
                    if tx["verboseData"].get("computeMass", "0") not in ("0", 0)
                    else None,
                    "payload": tx["payload"] if tx["payload"] else None,
                    "block_hash": block_hashes,
                    "block_time": tx["verboseData"]["blockTime"],
                    "inputs": [
                        {
                            "transaction_id": tx["verboseData"]["transactionId"],
                            "index": tx_in_idx,
                            "previous_outpoint_hash": tx_in["previousOutpoint"]["transactionId"],
                            "previous_outpoint_index": tx_in["previousOutpoint"]["index"],
                            "signature_script": tx_in["signatureScript"],
                            "sig_op_count": tx_in["sigOpCount"],
                        }
                        for tx_in_idx, tx_in in enumerate(tx["inputs"])
                    ]
                    if include_inputs and tx["inputs"]
                    else None,
                    "outputs": [
                        {
                            "transaction_id": tx["verboseData"]["transactionId"],
                            "index": tx_out_idx,
                            "amount": tx_out["amount"],
                            "script_public_key": tx_out["scriptPublicKey"]["scriptPublicKey"],
                            "script_public_key_address": tx_out["verboseData"]["scriptPublicKeyAddress"],
                            "script_public_key_type": tx_out["verboseData"]["scriptPublicKeyType"],
                        }
                        for tx_out_idx, tx_out in enumerate(tx["outputs"])
                    ]
                    if include_outputs and tx["outputs"]
                    else None,
                }
