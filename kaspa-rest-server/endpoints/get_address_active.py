# encoding: utf-8
from typing import List

from fastapi import HTTPException
from kaspa_script_address import to_script, to_address
from pydantic import BaseModel, Field
from sqlalchemy import values, column, func
from sqlalchemy.future import select

from constants import ADDRESS_EXAMPLE
from constants import USE_SCRIPT_FOR_ADDRESS, ADDRESS_PREFIX
from dbsession import async_session
from endpoints import sql_db_only
from models.TxAddrMapping import TxAddrMapping, TxScriptMapping
from server import app


class AddressesActiveRequest(BaseModel):
    addresses: list[str] = [ADDRESS_EXAMPLE]


class AddressesActiveResponse(BaseModel):
    address: str = Field(example="kaspa:qqkqkzjvr7zwxxmjxjkmxxdwju9kjs6e9u82uh59z07vgaks6gg62v8707g73")
    active: bool = Field(example=True)
    lastTxBlockTime: int | None = Field(example=1752924174352)


@app.post(
    "/addresses/active",
    response_model=List[AddressesActiveResponse],
    response_model_exclude_unset=True,
    tags=["Kaspa addresses"],
    openapi_extra={"strict_query_params": True},
)
@sql_db_only
async def get_addresses_active(addresses_active_request: AddressesActiveRequest):
    """
    This endpoint checks if addresses have had any transaction activity in the past.
    It is specifically designed for HD Wallets to verify historical address activity.
    """
    async with async_session() as s:
        addresses = set(addresses_active_request.addresses)
        script_addresses = set()
        for address in addresses:
            try:
                script_addresses.add(to_script(address))
            except ValueError:
                raise HTTPException(status_code=400, detail=f"Invalid address: {address}")

        if USE_SCRIPT_FOR_ADDRESS:
            v = values(column("script_public_key", TxScriptMapping.__table__.c.script_public_key.type), name="v").data(
                [(addr,) for addr in script_addresses]
            )
            result = await s.execute(
                select(v.c.script_public_key, func.max(TxScriptMapping.block_time).label("last_tx"))
                .join(TxScriptMapping, TxScriptMapping.script_public_key == v.c.script_public_key)
                .group_by(v.c.script_public_key)
            )
            addresses_used = {to_address(ADDRESS_PREFIX, row.script_public_key): row.last_tx for row in result}
        else:
            v = values(column("address", TxAddrMapping.__table__.c.address.type), name="v").data(
                [(addr,) for addr in addresses]
            )
            result = await s.execute(
                select(v.c.address, func.max(TxScriptMapping.block_time).label("last_tx"))
                .join(TxAddrMapping, TxAddrMapping.address == v.c.address)
                .group_by(v.c.address)
            )
            addresses_used = {row.address: row.last_tx for row in result}

    return [
        AddressesActiveResponse(
            address=address, active=(address in addresses_used), lastTxBlockTime=addresses_used.get(address)
        )
        for address in addresses_active_request.addresses
    ]
