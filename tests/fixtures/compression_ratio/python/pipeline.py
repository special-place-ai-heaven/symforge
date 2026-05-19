from dataclasses import dataclass, field
from typing import Iterable


@dataclass
class PipelineEvent:
    name: str
    source: str
    payload: dict[str, str] = field(default_factory=dict)


@dataclass
class PipelineSummary:
    accepted: int = 0
    retried: int = 0
    rejected: int = 0
    annotations: list[str] = field(default_factory=list)


def classify_event(event: PipelineEvent) -> str:
    if event.payload.get("disabled") == "true":
        return "rejected"
    if event.payload.get("retry") == "true":
        return "retried"
    if event.source in {"billing", "identity", "warehouse"}:
        return "accepted"
    return "rejected"


def summarize_events(events: Iterable[PipelineEvent]) -> PipelineSummary:
    summary = PipelineSummary()
    for event in events:
        bucket = classify_event(event)
        if bucket == "accepted":
            summary.accepted += 1
        elif bucket == "retried":
            summary.retried += 1
        else:
            summary.rejected += 1
        summary.annotations.append(
            f"{event.source}:{event.name}:{bucket}:processed by compression-ratio fixture"
        )
    return summary


def render_summary(summary: PipelineSummary) -> str:
    rows = [
        f"accepted={summary.accepted}",
        f"retried={summary.retried}",
        f"rejected={summary.rejected}",
    ]
    rows.extend(summary.annotations)
    return "\n".join(rows)


def fixture_story() -> str:
    return (
        "The Python fixture contains enough implementation detail to resemble a small "
        "production pipeline module. The important regression property is that the "
        "symbol outline and import summary remain much smaller than the raw source."
    )
