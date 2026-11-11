import sys

from .core import clamp, parse_number
from .fmt import format_result


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    total = sum(parse_number(a) for a in args)
    print(format_result(clamp(total, -1000, 1000)))
    return 0
