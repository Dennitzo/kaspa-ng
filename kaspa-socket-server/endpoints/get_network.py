# encoding: utf-8

from server import kaspad_client


async def get_network():
    """
    Get some global kaspa network information
    """
    resp = await kaspad_client.request("getBlockDagInfoRequest")
    return resp["getBlockDagInfoResponse"]
