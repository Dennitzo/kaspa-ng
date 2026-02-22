# encoding: utf-8
import threading

from fastapi_utils.tasks import repeat_every

from endpoints.get_blockdag import get_blockdag
from server import sio, app

BLOCKS_CACHE = []


def room_has_clients(room_name: str) -> bool:
    try:
        namespace_rooms = sio.manager.rooms.get("/", {})
        clients = namespace_rooms.get(room_name)
        return bool(clients)
    except Exception:
        return False


@app.on_event("startup")
@repeat_every(seconds=5)
async def periodical_blockdag():
    if not room_has_clients("blockdag"):
        return
    await emit_blockdag()


async def emit_blockdag():
    resp = await get_blockdag()
    await sio.emit("blockdag", resp, room="blockdag")
