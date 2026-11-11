from .core import clamp


def format_result(value, digits=2):
    return f"{round(value, digits)}"


def bounded(value):
    return format_result(clamp(value, 0, 100))
