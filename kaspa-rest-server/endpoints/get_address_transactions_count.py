# encoding: utf-8
import logging
from fastapi import Path, HTTPException
from kaspa_script_address import to_script
from pydantic import BaseModel
from sqlalchemy import func, text
from sqlalchemy.future import select
from starlette.responses import Response

from constants import ADDRESS_EXAMPLE, REGEX_KASPA_ADDRESS
from constants import USE_SCRIPT_FOR_ADDRESS
from dbsession import async_session
from endpoints import sql_db_only
from models.TxAddrMapping import TxAddrMapping, TxScriptMapping, TxScriptCount, TxAddrCount
from server import app

_logger = logging.getLogger(__name__)

_table_exists: bool | None = None


class TransactionCount(BaseModel):
    total: int


@app.get(
    "/addresses/{kaspaAddress}/transactions-count",
    response_model=TransactionCount,
    tags=["Kaspa addresses"],
    openapi_extra={"strict_query_params": True},
)
@sql_db_only
async def get_transaction_count_for_address(
    response: Response,
    kaspa_address: str = Path(
        alias="kaspaAddress", description=f"Kaspa address as string e.g. {ADDRESS_EXAMPLE}", regex=REGEX_KASPA_ADDRESS
    ),
):
    """
    Count the number of transactions associated with this address
    """
    try:
        script = to_script(kaspa_address)
    except ValueError:
        raise HTTPException(status_code=400, detail=f"Invalid address: {kaspa_address}")

    async with async_session() as s:
        global _table_exists
        if _table_exists is None:
            if USE_SCRIPT_FOR_ADDRESS:
                table_name = TxScriptCount.__tablename__
            else:
                table_name = TxAddrCount.__tablename__
            check_table_exists_sql = text(f"""
                    SELECT EXISTS (
                        SELECT FROM information_schema.tables 
                        WHERE table_schema = 'public' AND table_name = '{table_name}'
                    );
                """)
            result = await s.execute(check_table_exists_sql)
            _table_exists = result.scalar()
            if _table_exists:
                _logger.info(f"Per address tx count helper table {table_name} detected")
            else:
                _logger.info(f"Per address tx count helper table {table_name} NOT found")

        if _table_exists:
            if USE_SCRIPT_FOR_ADDRESS:
                result = await s.execute(select(TxScriptCount.count).filter(TxScriptCount.script_public_key == script))
            else:
                result = await s.execute(select(TxAddrCount.count).filter(TxAddrCount.address == kaspa_address))
            tx_count = result.scalar()
            ttl = 4
        else:
            if USE_SCRIPT_FOR_ADDRESS:
                result = await s.execute(select(func.count()).filter(TxScriptMapping.script_public_key == script))
            else:
                result = await s.execute(select(func.count()).filter(TxAddrMapping.address == kaspa_address))
            tx_count = result.scalar()

            if tx_count >= 1_000_000:
                ttl = 600
            elif tx_count >= 100_000:
                ttl = 60
            elif tx_count >= 10_000:
                ttl = 20
            else:
                ttl = 8

    response.headers["Cache-Control"] = f"public, max-age={ttl}"
    return TransactionCount(total=tx_count or 0)
