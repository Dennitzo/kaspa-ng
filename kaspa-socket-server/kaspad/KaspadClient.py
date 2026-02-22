# encoding: utf-8

import asyncio
import os

from kaspad.KaspadThread import KaspadThread


# pipenv run python -m grpc_tools.protoc -I./protos --python_out=. --grpc_python_out=. ./protos/rpc.proto ./protos/messages.proto ./protos/p2p.proto

class KaspadClient(object):
    def __init__(self, kaspad_host, kaspad_port):
        self.kaspad_host = kaspad_host
        self.kaspad_port = kaspad_port
        self.server_version = None
        self.is_utxo_indexed = None
        self.is_synced = None
        self.p2p_id = None
        self._pool = asyncio.Queue()
        self._pool_created = 0
        self._pool_max = int(os.getenv("KASPAD_POOL_SIZE", "2"))
        self._pool_lock = asyncio.Lock()

    async def ping(self):
        try:
            info = await self.request("getInfoRequest")
            self.server_version = info["getInfoResponse"]["serverVersion"]
            self.is_utxo_indexed = info["getInfoResponse"]["isUtxoIndexed"]
            self.is_synced = info["getInfoResponse"]["isSynced"]
            self.p2p_id = info["getInfoResponse"]["p2pId"]
            return info

        except Exception as exc:
            return False

    async def _acquire_thread(self):
        try:
            return self._pool.get_nowait()
        except asyncio.QueueEmpty:
            async with self._pool_lock:
                if self._pool_created < self._pool_max:
                    self._pool_created += 1
                    return KaspadThread(self.kaspad_host, self.kaspad_port, async_thread=True)
            return await self._pool.get()

    async def _release_thread(self, thread):
        try:
            self._pool.put_nowait(thread)
        except asyncio.QueueFull:
            try:
                await thread.close()
            except Exception:
                pass

    async def request(self, command, params=None, timeout=5):
        thread = await self._acquire_thread()
        discard_thread = False
        try:
            return await thread.request(command, params, wait_for_response=True, timeout=timeout)
        except Exception:
            discard_thread = True
            try:
                await thread.close()
            finally:
                raise
        finally:
            if not discard_thread:
                await self._release_thread(thread)

    async def notify(self, command, params, callback):
        t = KaspadThread(self.kaspad_host, self.kaspad_port, async_thread=True)
        try:
            return await t.notify(command, params, callback)
        finally:
            try:
                await t.close()
            except Exception:
                pass
