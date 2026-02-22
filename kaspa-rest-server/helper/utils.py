import time

from constants import BPS
from endpoints.get_virtual_chain_blue_score import current_blue_score_data


def add_cache_control(blue_score, timestamp, response):
    if blue_score and current_blue_score_data["blue_score"] > 0:
        delta_seconds = abs(current_blue_score_data["blue_score"] - int(blue_score)) / BPS
    elif timestamp:
        delta_seconds = abs(time.time() - int(timestamp) / 1000)
    else:
        return
    if delta_seconds < 20:  # <20 seconds
        ttl = 4  # 4 seconds
    elif delta_seconds < 60:  # 20s - 1m
        ttl = 20  # 20 seconds
    elif delta_seconds < 600:  # 1m - 10m
        ttl = 60  # 1 minute
    elif delta_seconds < 3600:  # 10m - 1h
        ttl = 600  # 10 minutes
    elif delta_seconds < 10800:  # 1h - 3h
        ttl = 1200  # 20 minutes
    elif delta_seconds < 36000:  # 3h - 10h
        ttl = 3600  # 1 hour
    elif delta_seconds < 86400:  # 10h - 1d
        ttl = 10800  # 3 hours
    elif delta_seconds < 172800:  # 1d - 2d
        ttl = 36000  # 10 hours
    else:  # > 2d
        ttl = 86400  # 1 day
    response.headers["Cache-Control"] = f"public, max-age={ttl}"
