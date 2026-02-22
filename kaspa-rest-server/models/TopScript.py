from kaspa_script_address import to_address
from sqlalchemy import Column, BigInteger, SmallInteger

from constants import ADDRESS_PREFIX
from dbsession import Base
from models.type_decorators.HexColumn import HexColumn


class TopScript(Base):
    __tablename__ = "top_scripts"
    timestamp = Column(BigInteger, primary_key=True)
    rank = Column(SmallInteger, primary_key=True)
    script_public_key = Column(HexColumn)
    amount = Column(BigInteger)

    @property
    def script_public_key_address(self):
        return to_address(ADDRESS_PREFIX, self.script_public_key)
