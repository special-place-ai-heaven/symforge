import unittest

from minicalc.core import mul


class MulTests(unittest.TestCase):
    def test_mul(self):
        self.assertEqual(mul(3, 4), 12)
        self.assertEqual(mul(-2, 5), -10)
