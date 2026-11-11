import unittest

from minicalc.core import Registry, add, clamp, parse_number
from minicalc.fmt import bounded, format_result


class CoreTests(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2, 3), 5)

    def test_clamp(self):
        self.assertEqual(clamp(5, 0, 10), 5)
        self.assertEqual(clamp(-1, 0, 10), 0)
        self.assertEqual(clamp(11, 0, 10), 10)

    def test_parse_number(self):
        self.assertEqual(parse_number(" 4 "), 4)
        self.assertEqual(parse_number("1.5"), 1.5)

    def test_registry(self):
        r = Registry()
        r.register("b", 2)
        r.register("a", 1)
        self.assertEqual(r.names(), ["a", "b"])

    def test_format(self):
        self.assertEqual(format_result(1.234), "1.23")
        self.assertEqual(bounded(150), "100")
