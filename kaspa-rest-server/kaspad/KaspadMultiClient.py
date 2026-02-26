# encoding: utf-8
import asyncio
import os
import time

from kaspad.KaspadClient import KaspadClient

# poetry run python -m grpc_tools.protoc -I./protos --python_out=. --grpc_python_out=. ./protos/rpc.proto ./protos/messages.proto
from kaspad.KaspadThread import KaspadCommunicationError


class KaspadMultiClient(object):
    def __init__(self, hosts: list[str]):
        self.kaspads = [KaspadClient(*h.split(":")) for h in hosts]
        self._reinit_cooldown_seconds = float(
            os.getenv("KASPAD_REINIT_COOLDOWN_SECONDS", "10")
        )
        self._last_initialize_at = 0.0
        self._initialize_lock = asyncio.Lock()

    def __get_kaspad(self):
        for k in self.kaspads:
            if k.is_utxo_indexed and k.is_synced:
                return k
        return None

    def __get_any_online_kaspad(self):
        for k in self.kaspads:
            if k.server_version:
                return k
        if self.kaspads:
            return self.kaspads[0]
        return None

    async def __get_ready_kaspad(self):
        kaspad = self.__get_kaspad()
        if kaspad is not None:
            return kaspad

        await self.initialize_all(force=False)
        kaspad = self.__get_kaspad()
        if kaspad is not None:
            return kaspad

        raise KaspadCommunicationError(
            "No available kaspad backend (requires synced node with utxoindex enabled)"
        )

    async def initialize_all(self, force=False):
        now = time.monotonic()
        if (
            not force
            and now - self._last_initialize_at < self._reinit_cooldown_seconds
        ):
            return False

        async with self._initialize_lock:
            now = time.monotonic()
            if (
                not force
                and now - self._last_initialize_at < self._reinit_cooldown_seconds
            ):
                return False

            tasks = [asyncio.create_task(k.ping()) for k in self.kaspads]

            for t in tasks:
                await t

            self._last_initialize_at = time.monotonic()
            return True

    async def request(self, command, params=None, timeout=5):
        try:
            kaspad = await self.__get_ready_kaspad()
            return await kaspad.request(command, params, timeout=timeout)
        except KaspadCommunicationError:
            await self.initialize_all(force=False)
            try:
                kaspad = await self.__get_ready_kaspad()
                return await kaspad.request(command, params, timeout=timeout)
            except KaspadCommunicationError as strict_error:
                # Fallback path:
                # Some RPC calls are still useful while syncing (e.g. blockdag / fee estimate / balances).
                # Prefer strict "synced+utxoindex" backends, but if none exist yet, try any reachable backend.
                last_error = strict_error
                online = self.__get_any_online_kaspad()
                candidates = [online] if online else []
                candidates.extend([k for k in self.kaspads if k not in candidates])

                for candidate in candidates:
                    try:
                        return await candidate.request(command, params, timeout=timeout)
                    except Exception as err:
                        last_error = err
                        continue

                raise KaspadCommunicationError(str(last_error))

    async def notify(self, command, params, callback):
        kaspad = await self.__get_ready_kaspad()
        return await kaspad.notify(command, params, callback)
