import pytest

from flask import Config


def test_get_bool():
    c = Config(".")
    c.update(A="true", B="0", C=True, D="Yes", E="off", F="", G=False, H="ON")
    assert c.get_bool("A") is True
    assert c.get_bool("B") is False
    assert c.get_bool("C") is True
    assert c.get_bool("D") is True
    assert c.get_bool("E") is False
    assert c.get_bool("F") is False
    assert c.get_bool("G") is False
    assert c.get_bool("H") is True
    assert c.get_bool("MISSING") is False
    assert c.get_bool("MISSING", True) is True


def test_get_bool_rejects_other_strings():
    c = Config(".")
    c["X"] = "maybe"
    with pytest.raises(ValueError):
        c.get_bool("X")
