# encoding: utf-8
from asyncio import wait_for
from typing import List

from fastapi import HTTPException
from pydantic import BaseModel, Field

from kaspad.KaspadRpcClient import kaspad_rpc_client
from server import app, kaspad_client


class BlockdagResponse(BaseModel):
    networkName: str = Field(..., example="kaspa-mainnet")
    blockCount: str = Field(..., example="260890")
    headerCount: str = Field(..., example="2131312")
    tipHashes: List[str] = Field(..., example=["78273854a739e3e379dfd34a262bbe922400d8e360e30e3f31228519a334350a"])
    difficulty: float = Field(..., example=3870677677777.2)
    pastMedianTime: str = Field(..., example="1656455670700")
    virtualParentHashes: List[str] = Field(
        ..., example=["78273854a739e3e379dfd34a262bbe922400d8e360e30e3f31228519a334350a"]
    )
    pruningPointHash: str = Field(..., example="5d32a9403273a34b6551b84340a1459ddde2ae6ba59a47987a6374340ba41d5d")
    virtualDaaScore: str = Field(..., example="19989141")
    sink: str = Field(..., example="366b1cf51146cc002672b79948634751a2914a2cc9e273afe358bdc1ae19dce9")


@app.get("/info/network", response_model=BlockdagResponse, tags=["Kaspa network info"], deprecated=True)
async def get_network():
    """
    Alias for /info/blockdag
    """
    return await get_blockdag()


@app.get("/info/blockdag", response_model=BlockdagResponse, tags=["Kaspa network info"])
async def get_blockdag():
    """
    Get Kaspa BlockDAG information
    """
    rpc_client = await kaspad_rpc_client()
    if rpc_client:
        info = await wait_for(rpc_client.get_block_dag_info(), 10)
        info["networkName"] = f"kaspa-{info['network']}"
        return info
    else:
        resp = await kaspad_client.request("getBlockDagInfoRequest")
        if "error" in resp:
            raise HTTPException(500, resp["error"])
        return resp["getBlockDagInfoResponse"]
