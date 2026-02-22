# encoding: utf-8

from fastapi_utils.tasks import repeat_every

from endpoints.get_info import get_info
from server import sio, app

mempool = 0


def room_has_clients(room_name: str) -> bool:
    try:
        namespace_rooms = sio.manager.rooms.get("/", {})
        clients = namespace_rooms.get(room_name)
        return bool(clients)
    except Exception:
        return False


@app.on_event("startup")
@repeat_every(seconds=5)
async def periodical_mempool():
    if not room_has_clients("mempool"):
        return
    await emit_mempool()


async def emit_mempool():
    global mempool
    resp = await get_info()
    mempool_size = resp.get("mempoolSize")
    if mempool_size is None:
        # Some responses can be partial while backend is still warming up.
        return

    if mempool_size != mempool:
        mempool = mempool_size
        await sio.emit("mempool", mempool, room="mempool")
