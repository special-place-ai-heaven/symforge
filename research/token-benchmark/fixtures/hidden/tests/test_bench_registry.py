import unittest

from minicalc.core import Registry
from minicalc.fmt import format_registry


class RegistryTests(unittest.TestCase):
    def test_unregister_and_format(self):
        r = Registry()
        r.register("b", 2)
        r.register("a", 1)
        r.register("c", 3)
        r.unregister("c")
        self.assertEqual(r.names(), ["a", "b"])
        self.assertEqual(format_registry(r), "a=1, b=2")
        with self.assertRaises(KeyError):
            r.unregister("missing")
