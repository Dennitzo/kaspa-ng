import logging
from collections.abc import Callable, Coroutine
from typing import Any

from fastapi import Request, Response
from fastapi.exceptions import RequestValidationError
from fastapi.routing import APIRoute

_logger = logging.getLogger(__name__)


class StrictRoute(APIRoute):
    def get_route_handler(self) -> Callable[[Request], Coroutine[Any, Any, Response]]:
        original_route_handler = super().get_route_handler()
        if self.openapi_extra is not None and "strict_query_params" in self.openapi_extra:
            known_params = [param.alias for param in self.dependant.query_params]

            async def custom_route_handler(request: Request) -> Response:
                unknown_params = [qp for qp in request.query_params if qp not in known_params]
                if any(unknown_params):
                    raise RequestValidationError(
                        errors=[
                            {
                                "loc": ["query", param],
                                "msg": f"Unknown query parameter in request: {param}",
                                "type": "value_error",
                                "input": f"{param}={request.query_params[param]}",
                                "ctx": {"available_parameters": known_params},
                            }
                            for param in unknown_params
                        ]
                    )
                return await original_route_handler(request)

            return custom_route_handler
        return original_route_handler
