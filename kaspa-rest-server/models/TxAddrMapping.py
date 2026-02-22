from kaspa_script_address import to_address
from sqlalchemy import Column, BigInteger

from constants import ADDRESS_PREFIX
from dbsession import Base
from models.type_decorators.HexColumn import HexColumn
from models.AddressColumn import AddressColumn


class TxAddrMapping(Base):
    __tablename__ = "addresses_transactions"
    transaction_id = Column(HexColumn, primary_key=True)
    address = Column(AddressColumn, primary_key=True)
    block_time = Column(BigInteger)


class TxAddrCount(Base):
    __tablename__ = "addresses_transactions_count"
    address = Column(AddressColumn, primary_key=True)
    count = Column(BigInteger)


class TxScriptMapping(Base):
    __tablename__ = "scripts_transactions"
    transaction_id = Column(HexColumn, primary_key=True)
    script_public_key = Column(HexColumn, primary_key=True)
    block_time = Column(BigInteger)

    @property
    def script_public_key_address(self):
        return to_address(ADDRESS_PREFIX, self.script_public_key)


class TxScriptCount(Base):
    __tablename__ = "scripts_transactions_count"
    script_public_key = Column(HexColumn, primary_key=True)
    count = Column(BigInteger)

    @property
    def script_public_key_address(self):
        return to_address(ADDRESS_PREFIX, self.script_public_key)
