export interface DashboardMetric {
  key: string;
  label: string;
  value: number;
  stale: boolean;
}

export interface DashboardSection {
  title: string;
  metrics: DashboardMetric[];
}

export class DashboardPresenter {
  private sections: DashboardSection[];

  constructor(sections: DashboardSection[]) {
    this.sections = sections;
  }

  visibleMetrics(): DashboardMetric[] {
    return this.sections.flatMap((section) =>
      section.metrics.filter((metric) => !metric.stale),
    );
  }

  renderRows(): string[] {
    return this.visibleMetrics().map((metric) => {
      const value = metric.value.toLocaleString("en-US", {
        maximumFractionDigits: 2,
      });
      return `${metric.key}\t${metric.label}\t${value}`;
    });
  }

  explain(): string {
    return [
      "This fixture models a dashboard presenter with interfaces, a class,",
      "a constructor, filtering logic, formatting logic, and enough prose in",
      "strings to make the raw source meaningfully larger than the outline.",
    ].join(" ");
  }
}

export function buildDefaultSections(): DashboardSection[] {
  return [
    {
      title: "operations",
      metrics: [
        { key: "queue_depth", label: "Queue depth", value: 42, stale: false },
        { key: "retry_rate", label: "Retry rate", value: 0.031, stale: false },
        { key: "old_latency", label: "Old latency", value: 812, stale: true },
      ],
    },
    {
      title: "finance",
      metrics: [
        { key: "gross_margin", label: "Gross margin", value: 0.73, stale: false },
        { key: "refund_rate", label: "Refund rate", value: 0.014, stale: false },
      ],
    },
  ];
}
