"""Core arithmetic helpers for the benchmark fixture."""


def add(a, b):
    return a + b


def sub(a, b):
    return a - b


def clamp(value, lo, hi):
    return max(lo, min(value, hi))


def parse_number(text):
    text = text.strip()
    return float(text) if "." in text else int(text)


class Registry:
    def __init__(self):
        self._items = {}

    def register(self, name, value):
        self._items[name] = value

    def get(self, name):
        return self._items[name]

    def names(self):
        return sorted(self._items)
