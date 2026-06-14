"""Minimal records-style corpus for STEL multi-hop golden replay."""


class Connection:
    """Reference target for multi-hop find_references replay."""

    def __init__(self) -> None:
        self.open = False


def open_connection() -> Connection:
    return Connection()


def reuse_connection(existing: Connection) -> Connection:
    return existing
