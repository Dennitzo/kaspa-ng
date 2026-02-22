# encoding: utf-8

from server import sio
from sockets.blockdag import emit_blockdag
from sockets.bluescore import emit_bluescore
from sockets.coinsupply import emit_coin_supply
from sockets.mempool import emit_mempool

VALID_ROOMS = ["blocks", "coinsupply", "blockdag", "bluescore", "mempool"]


def room_has_clients(room_name: str) -> bool:
    try:
        namespace_rooms = sio.manager.rooms.get("/", {})
        clients = namespace_rooms.get(room_name)
        return bool(clients)
    except Exception:
        return False


@sio.on("join-room")
async def join_room(sid, room_name):
    if room_name in VALID_ROOMS:
        print(f"{sid} joining {room_name}")
        sio.enter_room(sid, room_name)

        if room_name == "blockdag":
            await emit_blockdag()

        if room_name == "coinsupply":
            await emit_coin_supply()

        if room_name == "bluescore":
            await emit_bluescore()

        if room_name == "mempool":
            await emit_mempool()
