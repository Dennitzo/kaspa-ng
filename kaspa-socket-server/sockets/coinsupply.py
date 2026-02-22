# encoding: utf-8

from fastapi_utils.tasks import repeat_every

from endpoints.get_circulating_supply import get_coinsupply
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
@repeat_every(seconds=5, wait_first=True)
async def periodic_coin_supply():
    if not room_has_clients("coinsupply"):
        return
    await emit_coin_supply()


async def emit_coin_supply():
    resp = await get_coinsupply()
    await sio.emit("coinsupply", resp, room="coinsupply")
