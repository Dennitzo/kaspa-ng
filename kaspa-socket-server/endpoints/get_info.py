# encoding: utf-8

from server import kaspad_client


async def get_info():
    """
    Get some global Kaspa BlockDAG information
    """
    resp = await kaspad_client.request("getInfoRequest")
    return resp["getInfoResponse"]
