import flask.signals as signals
from flask import Flask


def test_config_loaded_signal():
    app = Flask(__name__)
    seen = []

    def receiver(sender, **kwargs):
        seen.append((sender, kwargs.get("keys")))

    signals.config_loaded.connect(receiver)
    try:
        app.config.from_mapping({"ALPHA": 1, "lower": 2}, BETA=3)
    finally:
        signals.config_loaded.disconnect(receiver)

    assert len(seen) == 1
    sender, keys = seen[0]
    assert sender is app.config
    assert keys == ["ALPHA", "BETA"]
    assert app.config["ALPHA"] == 1 and app.config["BETA"] == 3
    assert "lower" not in app.config
