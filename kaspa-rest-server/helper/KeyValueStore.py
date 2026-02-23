# encoding: utf-8
from sqlalchemy import insert, select, text, update

from dbsession import async_session
from models.Variable import KeyValueModel


async def _ensure_vars_table(session):
    await session.execute(
        text(
            """
            CREATE TABLE IF NOT EXISTS vars (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            """
        )
    )


async def get(key):
    async with async_session() as s:
        await _ensure_vars_table(s)
        result = await s.execute(select(KeyValueModel.value).where(KeyValueModel.key == key))
        return result.scalar()


async def set(key, value):
    async with async_session() as s:
        await _ensure_vars_table(s)
        result = await s.execute(update(KeyValueModel).where(KeyValueModel.key == key).values(value=value))

        if result.rowcount == 1:
            await s.commit()
            return True

        result = await s.execute(insert(KeyValueModel).values(key=key, value=value))

        await s.commit()

        return True
