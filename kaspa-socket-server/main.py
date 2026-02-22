# encoding: utf-8
import asyncio
import os
from asyncio import Task, CancelledError

from fastapi_utils.tasks import repeat_every
from starlette.responses import RedirectResponse

import sockets
from server import app, kaspad_client
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


@app.on_event("startup")
async def startup():
    global BLOCKS_TASK
    # find kaspad before staring webserver
    await kaspad_client.initialize_all()
    BLOCKS_TASK = _start_blocks_task()


@app.on_event("shutdown")
async def shutdown():
    global SHUTTING_DOWN, BLOCKS_TASK
    SHUTTING_DOWN = True
    if BLOCKS_TASK and not BLOCKS_TASK.done():
        BLOCKS_TASK.cancel()
        try:
            await BLOCKS_TASK
        except CancelledError:
            pass


@app.on_event("startup")
@repeat_every(seconds=5)
async def watchdog():
    global BLOCKS_TASK, SHUTTING_DOWN

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
        print(f"Watch found an error! {exception}\n"
              f"Reinitialize kaspads and start task again")
        await kaspad_client.initialize_all()
        BLOCKS_TASK = _start_blocks_task()


@app.get("/", include_in_schema=False)
async def docs_redirect():
    return RedirectResponse(url='/docs')


if __name__ == '__main__':
    if os.getenv("DEBUG"):
        import uvicorn

        uvicorn.run(app)
