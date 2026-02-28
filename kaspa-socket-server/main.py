# encoding: utf-8
import asyncio
import os
import time
from asyncio import Task, CancelledError

import socketio
from fastapi_utils.tasks import repeat_every
from starlette.responses import RedirectResponse

import sockets
from server import app as fastapi_app, kaspad_client, sio
from sockets import blocks
from sockets.blockdag import periodical_blockdag
from sockets.bluescore import periodical_blue_score
from sockets.coinsupply import periodic_coin_supply
from sockets.mempool import periodical_mempool

print(
    f"Loaded: {sockets.join_room}"
    f"{periodic_coin_supply} {periodical_blockdag} {periodical_blue_score} {periodical_mempool}")

BLOCKS_TASK = None  # type: Task
SHUTTING_DOWN = False
LAST_WATCHDOG_LOG_TS = 0.0


def _start_blocks_task() -> Task:
    task = asyncio.create_task(blocks.config())

    def _consume_task_result(done_task: Task):
        try:
            _ = done_task.exception()
        except CancelledError:
            return
        except Exception:
            return

    task.add_done_callback(_consume_task_result)
    return task


@fastapi_app.on_event("startup")
async def startup():
    global BLOCKS_TASK
    # find kaspad before staring webserver
    await kaspad_client.initialize_all()
    BLOCKS_TASK = _start_blocks_task()


@fastapi_app.on_event("shutdown")
async def shutdown():
    global SHUTTING_DOWN, BLOCKS_TASK
    SHUTTING_DOWN = True
    if BLOCKS_TASK and not BLOCKS_TASK.done():
        BLOCKS_TASK.cancel()
        try:
            await BLOCKS_TASK
        except CancelledError:
            pass


@fastapi_app.on_event("startup")
@repeat_every(seconds=5)
async def watchdog():
    global BLOCKS_TASK, SHUTTING_DOWN, LAST_WATCHDOG_LOG_TS

    if SHUTTING_DOWN or BLOCKS_TASK is None:
        return
    if not BLOCKS_TASK.done():
        return

    try:
        exception = BLOCKS_TASK.exception()
    except CancelledError:
        return
    except Exception:
        return
    else:
        if SHUTTING_DOWN:
            return
        now = time.monotonic()
        # Keep watchdog retries, but avoid noisy log spam while node is not ready.
        if now - LAST_WATCHDOG_LOG_TS >= 30:
            print(
                f"Watch: backend not ready yet ({exception}). Reinitializing kaspad clients and retrying..."
            )
            LAST_WATCHDOG_LOG_TS = now
        await kaspad_client.initialize_all()
        BLOCKS_TASK = _start_blocks_task()


@fastapi_app.get("/", include_in_schema=False)
async def docs_redirect():
    return RedirectResponse(url='/docs')


# Socket.IO endpoint is mounted as a top-level ASGI wrapper to avoid
# framework/version-dependent mount-path behavior.
app = socketio.ASGIApp(sio, other_asgi_app=fastapi_app, socketio_path="ws/socket.io")


if __name__ == '__main__':
    if os.getenv("DEBUG"):
        import uvicorn

        uvicorn.run(app)
