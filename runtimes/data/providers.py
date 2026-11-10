"""Read original JSON numbers without losing decimal precision."""

from decimal import Decimal
import json


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON object member")
    return result


def invalid_constant(_):
    raise ValueError("invalid JSON numeric constant")


def read_json(body):
    try:
        return json.loads(body, parse_float=Decimal, object_pairs_hook=unique_object,
                          parse_constant=invalid_constant)
    except RecursionError:
        raise ValueError("source JSON nesting limit exceeded") from None
