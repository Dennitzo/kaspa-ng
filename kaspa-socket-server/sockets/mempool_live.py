# encoding: utf-8

import json
import logging
import math
import time
from datetime import datetime, timezone
from typing import Any

from fastapi_utils.tasks import repeat_every

from endpoints.get_blockdag import get_blockdag
from server import app, kaspad_client, sio

MEMPOOL_LIVE_INTERVAL_SECONDS = 2
MEMPOOL_TILE_LIMIT = 120
MEMPOOL_WINDOW_SECONDS = 60

_last_payload_hash = None
_block_mass_limit = 1_000_000
_logger = logging.getLogger("mempool-live")
_window_entries: dict[str, dict[str, Any]] = {}


def _room_has_clients(room_name: str) -> bool:
    try:
        namespace_rooms = sio.manager.rooms.get("/", {})
        clients = namespace_rooms.get(room_name)
        return bool(clients)
    except Exception:
        return False


def _mempool_fee_rate_buckets() -> list[dict[str, Any]]:
    return [
        {"min": 0, "max": 1},
        {"min": 1, "max": 2},
        {"min": 2, "max": 5},
        {"min": 5, "max": 10},
        {"min": 10, "max": 20},
        {"min": 20, "max": 50},
        {"min": 50, "max": 100},
        {"min": 100, "max": 200},
        {"min": 200, "max": 500},
        {"min": 500, "max": 1000},
        {"min": 1000, "max": 2000},
        {"min": 2000, "max": None},
    ]


def _bucket_for_fee_rate(rate: float, buckets: list[dict[str, Any]]) -> int:
    for idx, bucket in enumerate(buckets):
        min_val = float(bucket["min"])
        max_val = bucket.get("max")
        if max_val is None:
            if rate >= min_val:
                return idx
        else:
            if rate >= min_val and rate < float(max_val):
                return idx
    return len(buckets) - 1


def _percentile(values: list[float], percentile: float) -> float | None:
    if not values:
        return None
    if percentile <= 0:
        return min(values)
    if percentile >= 100:
        return max(values)
    sorted_vals = sorted(values)
    rank = (percentile / 100) * (len(sorted_vals) - 1)
    lower = int(math.floor(rank))
    upper = int(math.ceil(rank))
    if lower == upper:
        return sorted_vals[lower]
    weight = rank - lower
    return sorted_vals[lower] * (1 - weight) + sorted_vals[upper] * weight


def _int_value(value: Any, default: int = 0) -> int:
    if value is None:
        return default
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def _extract_tx_id(tx: dict[str, Any] | None) -> str | None:
    if not isinstance(tx, dict):
        return None
    for key in ("transactionId", "transaction_id", "hash", "id", "txId"):
        value = tx.get(key)
        if value:
            return str(value)
    verbose = tx.get("verboseData") or {}
    for key in ("transactionId", "transaction_id", "hash"):
        value = verbose.get(key)
        if value:
            return str(value)
    return None


async def _fetch_mempool_entries() -> list[dict[str, Any]]:
    try:
        resp = await kaspad_client.request(
            "getMempoolEntriesRequest",
            {
                "includeOrphanPool": False,
                "filterTransactionPool": False,
            },
            timeout=10,
        )
    except Exception:
        return []

    response = resp.get("getMempoolEntriesResponse") if isinstance(resp, dict) else None
    entries = response.get("entries") if isinstance(response, dict) else None
    if not isinstance(entries, list):
        return []

    normalized: list[dict[str, Any]] = []
    for entry in entries:
        fee = _int_value(entry.get("fee"))
        tx = entry.get("transaction") or {}
        verbose = tx.get("verboseData") or {}
        mass = _int_value(verbose.get("mass"))
        if mass <= 0:
            continue
        tx_id = _extract_tx_id(tx)
        if not tx_id:
            continue
        fee_rate = fee / mass if mass else 0
        normalized.append(
            {
                "id": tx_id,
                "fee": fee,
                "mass": mass,
                "feeRate": fee_rate,
            }
        )
    return normalized


def _build_mempool_snapshot(entries: list[dict[str, Any]]) -> dict[str, Any]:
    now = datetime.now(timezone.utc).isoformat()
    if not entries:
        return {
            "pending": True,
            "capturedAt": now,
            "txCount": 0,
            "totalMass": 0,
            "totalFee": 0,
            "feeRateMin": None,
            "feeRateMedian": None,
            "feeRateP90": None,
            "feeRateMax": None,
            "buckets": [],
            "tiles": [],
            "aggregates": {},
            "blockMassLimit": _block_mass_limit,
        }

    buckets = _mempool_fee_rate_buckets()
    bucket_stats = [{"min": b["min"], "max": b.get("max"), "count": 0, "mass": 0, "fee": 0} for b in buckets]
    fee_rates: list[float] = []
    enriched: list[dict[str, Any]] = []
    total_fee = 0
    total_mass = 0

    for entry in entries:
        fee = _int_value(entry.get("fee"))
        mass = _int_value(entry.get("mass"))
        if mass <= 0:
            continue
        fee_rate = entry.get("feeRate")
        if fee_rate is None:
            fee_rate = fee / mass if mass else 0
        fee_rates.append(fee_rate)
        total_fee += fee
        total_mass += mass
        enriched.append(
            {
                "id": entry.get("id"),
                "fee": fee,
                "mass": mass,
                "feeRate": fee_rate,
                "confirmed": not bool(entry.get("inMempool", True)),
            }
        )
        idx = _bucket_for_fee_rate(fee_rate, buckets)
        bucket_stats[idx]["count"] += 1
        bucket_stats[idx]["mass"] += mass
        bucket_stats[idx]["fee"] += fee

    if total_mass <= 0:
        return {
            "pending": True,
            "capturedAt": now,
            "txCount": 0,
            "totalMass": 0,
            "totalFee": 0,
            "feeRateMin": None,
            "feeRateMedian": None,
            "feeRateP90": None,
            "feeRateMax": None,
            "buckets": bucket_stats,
            "tiles": [],
            "aggregates": {},
            "blockMassLimit": _block_mass_limit,
        }

    enriched.sort(key=lambda row: (row.get("feeRate") or 0, row.get("mass") or 0), reverse=True)
    tiles = enriched[: max(1, MEMPOOL_TILE_LIMIT)]
    tiles_mass = sum(tile.get("mass") or 0 for tile in tiles)
    fee_rate_min = min(fee_rates) if fee_rates else None
    fee_rate_median = _percentile(fee_rates, 50) if fee_rates else None
    fee_rate_p90 = _percentile(fee_rates, 90) if fee_rates else None
    fee_rate_max = max(fee_rates) if fee_rates else None

    return {
        "pending": False,
        "capturedAt": now,
        "txCount": len(entries),
        "totalMass": total_mass,
        "totalFee": total_fee,
        "feeRateMin": fee_rate_min,
        "feeRateMedian": fee_rate_median,
        "feeRateP90": fee_rate_p90,
        "feeRateMax": fee_rate_max,
        "buckets": bucket_stats,
        "tiles": tiles,
        "aggregates": {
            "remainingCount": max(0, len(enriched) - len(tiles)),
            "remainingMass": max(0, total_mass - tiles_mass),
            "feeRateMin": fee_rate_min,
            "feeRateMedian": fee_rate_median,
            "feeRateP90": fee_rate_p90,
            "feeRateMax": fee_rate_max,
        },
        "blockMassLimit": _block_mass_limit,
    }


async def emit_mempool_live(force: bool = False) -> None:
    global _last_payload_hash, _window_entries
    if not force and not _room_has_clients("mempool-live"):
        return

    now_ts = time.time()
    cutoff = now_ts - MEMPOOL_WINDOW_SECONDS
    if _window_entries:
        for entry in _window_entries.values():
            entry["inMempool"] = False

    latest_entries = await _fetch_mempool_entries()
    for entry in latest_entries:
        entry_id = entry.get("id")
        if not entry_id:
            continue
        _window_entries[entry_id] = {
            "id": entry_id,
            "fee": entry.get("fee"),
            "mass": entry.get("mass"),
            "feeRate": entry.get("feeRate"),
            "lastSeen": now_ts,
            "inMempool": True,
        }

    if _window_entries:
        _window_entries = {key: value for key, value in _window_entries.items() if value.get("lastSeen", 0) >= cutoff}

    snapshot = _build_mempool_snapshot(list(_window_entries.values()))
    payload_str = json.dumps(snapshot, sort_keys=True, separators=(",", ":"))
    if force or payload_str != _last_payload_hash:
        _last_payload_hash = payload_str
        try:
            _logger.info(
                "emit mempool-live txs=%s mass=%s pending=%s window=%ss",
                snapshot.get("txCount"),
                snapshot.get("totalMass"),
                snapshot.get("pending"),
                MEMPOOL_WINDOW_SECONDS,
            )
        except Exception:
            pass
        await sio.emit("mempool-live", snapshot, room="mempool-live")


@app.on_event("startup")
@repeat_every(seconds=60)
async def _refresh_block_mass_limit():
    global _block_mass_limit
    try:
        info = await get_blockdag()
        if not isinstance(info, dict):
            return
        value = _int_value(info.get("blockMassLimit"), _block_mass_limit)
        if value > 0:
            _block_mass_limit = value
    except Exception:
        pass


@app.on_event("startup")
@repeat_every(seconds=MEMPOOL_LIVE_INTERVAL_SECONDS)
async def periodical_mempool_live():
    await emit_mempool_live()
