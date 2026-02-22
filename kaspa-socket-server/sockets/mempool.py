# encoding: utf-8

from fastapi_utils.tasks import repeat_every

from endpoints.get_info import get_info
from server import sio, app

mempool = 0


@app.on_event("startup")
@repeat_every(seconds=1)
async def periodical_mempool():
    await emit_mempool()


async def emit_mempool():
    global mempool
    resp = await get_info()

    if resp["mempoolSize"] != mempool:
        mempool = resp["mempoolSize"]
        await sio.emit("mempool", mempool, room="mempool")
