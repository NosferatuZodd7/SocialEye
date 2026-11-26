import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Stat } from "../components/ui";
import { toRule, describe as describeRule } from "../pages/Automations";
import { monthGrid } from "../pages/Calendar";
import { performance } from "../pages/PlatformPage";
import { audienceFor } from "../pages/Dashboard";
import { translate } from "../lib/i18n";
import type { Account, ContentItem } from "../lib/api";

describe("Stat", () => {
  it("shows unavailable data as hatched text, not zero", () => {
    const { container } = render(<Stat label="Followers" value={null} reason="not exposed by API" />);
    expect(screen.getByText("Not available")).toBeInTheDocument();
    expect(screen.getByText("not exposed by API")).toBeInTheDocument();
    expect(container.firstElementChild).toHaveClass("hatched");
    expect(screen.queryByText("0")).toBeNull();
  });
  it("shows value and source", () => {
    render(<Stat label="Subscribers" value={1500} source="YouTube Data API v3" />);
    expect(screen.getByText("1,500")).toBeInTheDocument();
    expect(screen.getByText(/YouTube Data API v3/)).toBeInTheDocument();
  });
});

describe("automation rules", () => {
  const base = { metric: "views", value: 10000, days: 7, platforms: [], inApp: true, desktop: false };
  it("sends only the fields a trigger uses", () => {
    expect(toRule({ ...base, type: "new_content" }).trigger).toEqual({ type: "new_content" });
    expect(toRule({ ...base, type: "content_threshold" }).trigger).toEqual({ type: "content_threshold", metric: "views", value: 10000 });
    expect(toRule({ ...base, type: "token_expiring" }).trigger).toEqual({ type: "token_expiring", days: 7 });
  });
  it("describes rules", () => {
    const t = Object.assign((k: Parameters<typeof translate>[1], v?: Record<string, string | number>) => translate("en", k, v), { lang: "en" as const });
    expect(describeRule(toRule({ ...base, type: "content_threshold" }), t)).toBe("A post or video reaches a value: Views ≥ 10,000");
  });
});

describe("helpers", () => {
  it("calendar grid starts on Monday and covers 6 weeks", () => {
    const g = monthGrid(2026, 1); // Feb 2026 starts on Sunday
    expect(g).toHaveLength(42);
    expect(g[0].getUTCDay()).toBe(1);
    expect(g[0].toISOString().slice(0, 10)).toBe("2026-01-26");
  });
  it("performance uses each platform's own metric", () => {
    expect(performance({ platform: "youtube", stats_json: '{"views":10,"likes":3}' } as ContentItem)).toBe(10);
    expect(performance({ platform: "instagram", stats_json: '{"likes":3,"comments":2}' } as ContentItem)).toBe(5);
    expect(performance({ platform: "spotify", stats_json: "{}" } as ContentItem)).toBeNull();
  });
  it("audience reports the unavailable reason", () => {
    const a = { id: 1, platform: "spotify", unavailable: "followers, plays: not exposed" } as Account;
    expect(audienceFor(a, [])).toMatchObject({ metric: "followers", point: null, reason: "followers, plays: not exposed" });
  });
});

describe("Toggle", () => {
  it("is an accessible switch", async () => {
    const { Toggle } = await import("../components/ui");
    const on = vi.fn();
    render(<Toggle checked={false} onChange={on} label="Auto sync" />);
    await userEvent.click(screen.getByRole("switch", { name: "Auto sync" }));
    expect(on).toHaveBeenCalledWith(true);
  });
});
