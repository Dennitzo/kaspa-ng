# encoding: utf-8

from server import kaspad_client


async def get_blockdag():
    """
    Get some global Kaspa BlockDAG information
    """
    resp = await kaspad_client.request("getBlockDagInfoRequest")
    return resp["getBlockDagInfoResponse"]
