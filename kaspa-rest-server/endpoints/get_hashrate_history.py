# encoding: utf-8
import calendar
import logging
from datetime import datetime, timezone
from typing import Optional

from fastapi import HTTPException
from fastapi.params import Query
from fastapi_utils.tasks import repeat_every
from pydantic import BaseModel
from sqlalchemy import select, text, func
from starlette.responses import Response

from fastapi import Path
from constants import (
    HASHRATE_HISTORY,
    A_DAY_MS,
    GENESIS_START_OF_DAY_MS,
    AN_HOUR_MS,
    REGEX_DATE_OPTIONAL_DAY,
    GENESIS_START_OF_MONTH_MS,
    CRESCENDO_BS,
)
from dbsession import async_session_blocks
from endpoints.get_virtual_chain_blue_score import get_virtual_selected_parent_blue_score
from helper.difficulty_calculation import bits_to_difficulty
from models.Block import Block
from models.HashrateHistory import HashrateHistory
from server import app

_logger = logging.getLogger(__name__)


class HashrateHistoryResponse(BaseModel):
    daaScore: int
    blueScore: int
    timestamp: int
    date_time: str
    bits: int | None
    difficulty: int
    hashrate_kh: int


_sample_interval_minutes = 15


@app.get(
    "/info/hashrate/history/{day_or_month}",
    response_model=list[HashrateHistoryResponse],
    tags=["Kaspa network info"],
)
async def get_hashrate_history_for_day_or_month(
    response: Response,
    day_or_month: str = Path(pattern=REGEX_DATE_OPTIONAL_DAY),
    resolution: Optional[str] = Query(default=None, enum=["15m", "1h"]),
):
    """
    Get hashrate history for a specific UTC day (YYYY-MM-DD) or month (YYYY-MM)
    """
    if not HASHRATE_HISTORY:
        raise HTTPException(status_code=503, detail="Hashrate history is disabled")

    now = datetime.now(tz=timezone.utc)
    now_ms = now.timestamp() * 1000

    response.headers["Cache-Control"] = "public, max-age=300"

    if len(day_or_month) == 10:
        dt = datetime.strptime(day_or_month, "%Y-%m-%d").replace(tzinfo=timezone.utc)
        start_ms = int(dt.timestamp() * 1000)
        if start_ms < GENESIS_START_OF_DAY_MS or start_ms > now_ms:
            return []
        end_ms = start_ms + A_DAY_MS
    else:
        dt = datetime.strptime(day_or_month, "%Y-%m").replace(tzinfo=timezone.utc)
        start_ms = int(dt.timestamp() * 1000)
        _, days_in_month = calendar.monthrange(dt.year, dt.month)
        end_ms = start_ms + days_in_month * A_DAY_MS
        if start_ms < GENESIS_START_OF_MONTH_MS or start_ms > now_ms:
            return []

    if end_ms < now_ms - 2 * A_DAY_MS:
        response.headers["Cache-Control"] = "public, max-age=3600"
    elif end_ms < now_ms - 2 * AN_HOUR_MS:
        response.headers["Cache-Control"] = "public, max-age=600"

    resolution_map = {
        None: int(15 / _sample_interval_minutes),
        "15m": int(15 / _sample_interval_minutes),
        "1h": int(60 / _sample_interval_minutes),
    }
    sample_interval = resolution_map.get(resolution)
    if not sample_interval:
        raise HTTPException(status_code=400, detail=f"Invalid resolution, allowed: {list(resolution_map.keys())}")

    async with async_session_blocks() as s:
        result = await s.execute(
            select(HashrateHistory)
            .where(HashrateHistory.timestamp >= start_ms)
            .where(HashrateHistory.timestamp < end_ms)
            .order_by(HashrateHistory.daa_score.desc())
        )
        samples = result.scalars().all()
        return filter_samples(samples, sample_interval)


@app.get("/info/hashrate/history", response_model=list[HashrateHistoryResponse], tags=["Kaspa network info"])
async def get_hashrate_history(
    response: Response, resolution: Optional[str] = Query(default=None, enum=["15m", "1h", "3h", "1d", "7d"])
):
    """
    Get historical hashrate samples with optional resolution (default = 1h)
    """
    if not HASHRATE_HISTORY:
        raise HTTPException(status_code=503, detail="Hashrate history is disabled")

    response.headers["Cache-Control"] = "public, max-age=3600"

    resolution_map = {
        None: int(60 / _sample_interval_minutes),
        "15m": int(15 / _sample_interval_minutes),
        "1h": int(60 / _sample_interval_minutes),
        "3h": int(3 * 60 / _sample_interval_minutes),
        "1d": int(24 * 60 / _sample_interval_minutes),
        "7d": int(7 * 24 * 60 / _sample_interval_minutes),
    }
    sample_interval = resolution_map.get(resolution)
    if not sample_interval:
        raise HTTPException(status_code=400, detail=f"Invalid resolution, allowed: {list(resolution_map.keys())}")

    async with async_session_blocks() as s:
        result = await s.execute(select(HashrateHistory).order_by(HashrateHistory.daa_score.desc()))
        samples = result.scalars().all()
        return filter_samples(samples, sample_interval)


def filter_samples(samples: list[HashrateHistory], sample_interval: int) -> list[HashrateHistoryResponse]:
    samples_filtered = []
    for i in range(0, len(samples), sample_interval):
        chunk = samples[i : i + sample_interval]
        first = chunk[-1]
        last = chunk[0]
        # If sampling and crossing the crescendo activation, we must create one sample before and one after
        # Otherwise there will be artifacts produced in the graph due to the sudden reduction in difficulty
        if first.blue_score < CRESCENDO_BS <= last.blue_score:
            difficulty = int(bits_to_difficulty(first.bits))
            hashrate_kh = difficulty * 2 // 1_000
            samples_filtered.append(hashrate_history(first, None, difficulty, hashrate_kh))
            difficulty = int(bits_to_difficulty(last.bits))
            hashrate_kh = difficulty * 2 * 10 // 1_000
            samples_filtered.append(hashrate_history(last, None, difficulty, hashrate_kh))
        else:
            bits = last.bits if sample_interval == 1 else None
            difficulty = int(sum(bits_to_difficulty(s.bits) for s in chunk) / len(chunk))
            hashrate_kh = difficulty * 2 * (1 if last.blue_score < CRESCENDO_BS else 10) // 1_000
            samples_filtered.append(hashrate_history(last, bits, difficulty, hashrate_kh))
    return samples_filtered


def hashrate_history(sample, bits, difficulty, hashrate_kh):
    return {
        "daaScore": sample.daa_score,
        "blueScore": sample.blue_score,
        "timestamp": sample.timestamp,
        "date_time": datetime.fromtimestamp(sample.timestamp / 1000, tz=timezone.utc).isoformat(
            timespec="milliseconds"
        ),
        "bits": bits,
        "difficulty": difficulty,
        "hashrate_kh": hashrate_kh,
    }


async def create_hashrate_history_table():
    if not HASHRATE_HISTORY:
        _logger.debug("Hashrate history: Disabled. Skipping table creation")
        return

    async with async_session_blocks() as s:
        await acquire_hashrate_history_lock(s)
        try:
            check_table_exists_sql = text(f"""
                SELECT EXISTS (
                    SELECT FROM information_schema.tables 
                    WHERE table_schema = 'public' AND table_name = '{HashrateHistory.__tablename__}'
                );
            """)
            result = await s.execute(check_table_exists_sql)
            if result.scalar():
                _logger.debug("Hashrate history: Table already exists")
                return

            _logger.warning("Hashrate history table does not exist, attempting to create it")
            create_table_sql = f"""
                CREATE TABLE IF NOT EXISTS {HashrateHistory.__tablename__} (
                    daa_score BIGINT PRIMARY KEY,
                    blue_score BIGINT,
                    timestamp BIGINT,
                    bits BIGINT
                );
            """
            await s.execute(text(create_table_sql))
            create_index_sql = f"""
                CREATE INDEX IF NOT EXISTS {HashrateHistory.__tablename__}_blue_score_idx ON {HashrateHistory.__tablename__} (blue_score);
            """
            await s.execute(text(create_index_sql))
            create_index_sql = f"""
                CREATE INDEX IF NOT EXISTS {HashrateHistory.__tablename__}_timestamp_idx ON {HashrateHistory.__tablename__} (timestamp);
            """
            await s.execute(text(create_index_sql))
            await s.commit()
        except Exception as e:
            _logger.exception(e)
            _logger.error(
                f"Hashrate history: Failed to create table, create it manually: \n{create_table_sql}{create_index_sql}"
            )
        finally:
            await release_hashrate_history_lock(s)


@repeat_every(seconds=600)
async def update_hashrate_history():
    batch_size = 1000

    if not HASHRATE_HISTORY:
        _logger.debug("Hashrate history: Disabled. Skipping update")
        return

    _logger.info("Hashrate history: Sampling hashrate history")
    sample_count = 0
    batch = []
    async with async_session_blocks() as s:
        await acquire_hashrate_history_lock(s)
        try:
            result = await s.execute(select(func.max(HashrateHistory.blue_score)))
            max_blue_score = result.scalar_one_or_none() or 0
            bps = 1 if max_blue_score < CRESCENDO_BS else 10  # Crescendo
            next_blue_score = 0
            if max_blue_score > 0:
                next_blue_score = max_blue_score + (bps * 60 * _sample_interval_minutes)

            current_blue_score = await get_virtual_selected_parent_blue_score()
            while int(current_blue_score["blueScore"]) > next_blue_score:
                result = await s.execute(
                    select(Block)
                    .where(Block.blue_score > next_blue_score)
                    .order_by(Block.blue_score, Block.daa_score)
                    .limit(1 if next_blue_score > 1236000 else 2)  # blue_score was reset 2021-11-22
                )
                blocks = result.scalars().all()
                if not blocks:
                    break
                if len(blocks) == 2:
                    if abs(blocks[0].daa_score - blocks[1].daa_score) < 100_000:
                        blocks = [blocks[0]]  # keep both only if they are on different sides of the reset
                for block in blocks:
                    if block.blue_score and block.daa_score and block.timestamp and block.bits:
                        batch.append(
                            HashrateHistory(
                                blue_score=block.blue_score,
                                daa_score=block.daa_score,
                                timestamp=block.timestamp,
                                bits=block.bits,
                            )
                        )
                        if len(batch) >= batch_size:
                            s.add_all(batch)
                            await s.commit()
                            batch.clear()
                        sample_count += 1
                        t = datetime.fromtimestamp(block.timestamp / 1000, tz=timezone.utc).isoformat(
                            timespec="seconds"
                        )
                        _logger.info(
                            f"Sampled: daa={block.daa_score}, blue_score={block.blue_score}, timestamp={t}, bits={block.bits}"
                        )
                block = blocks[0]
                bps = 1 if block.blue_score < CRESCENDO_BS else 10
                next_blue_score = block.blue_score + (bps * 60 * _sample_interval_minutes)
            if batch:
                s.add_all(batch)
                await s.commit()
        except Exception as e:
            _logger.exception(e)
            _logger.error("Hashrate history: Sampling failed")
        finally:
            await release_hashrate_history_lock(s)
    _logger.info(f"Hashrate history: Sampling complete, {sample_count} samples committed")


async def acquire_hashrate_history_lock(s):
    try:
        result = await s.execute(text("SELECT pg_try_advisory_lock(123100)"))
        if not result.scalar():
            _logger.debug("Hashrate history: waiting for advisory lock")
            await s.execute(text("SELECT pg_advisory_lock(123100)"))
        _logger.debug("Hashrate history: Acquired advisory lock (123100)")
    except Exception as e:
        _logger.exception(e)
        _logger.error("Hashrate history: unable to acquire advisory lock (123100)")
        raise e


async def release_hashrate_history_lock(s):
    await s.execute(text("SELECT pg_advisory_unlock(123100)"))
