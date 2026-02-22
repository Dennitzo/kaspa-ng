from sqlalchemy import Column, BigInteger, SmallInteger

from dbsession import Base


class DistributionTier(Base):
    __tablename__ = "distribution_tiers"
    timestamp = Column(BigInteger, primary_key=True)
    tier = Column(SmallInteger, primary_key=True)
    count = Column(BigInteger)
    amount = Column(BigInteger)
