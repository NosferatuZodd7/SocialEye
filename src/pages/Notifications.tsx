import { api } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { fmtRelative } from "../lib/format";
import { Button, Empty, ErrorBox, PageHeader, Panel, PlatformDot, cx } from "../components/ui";

export default function Notifications() {
  const t = useT();
  const { reloadUnread } = useStore();
  const list = useLoad(() => api.notifications(), []);
  const act = useAction();
  const run = (fn: () => Promise<void>) => act.run(async () => { await fn(); await list.reload(); await reloadUnread(); });
  return (
    <>
      <PageHeader title={t("no.title")} actions={<>
        <Button onClick={() => run(() => api.markRead())}>{t("no.markAll")}</Button>
        <Button variant="ghost" onClick={() => run(() => api.clearRead())}>{t("no.clear")}</Button>
      </>} />
      {(list.error || act.error) && <ErrorBox message={(list.error || act.error)!} onRetry={list.reload} />}
      <Panel pad={false}>
        {!list.data?.length ? <div className="p-4"><Empty title={t("no.empty")} /></div> : (
          <ul className="divide-y divide-[var(--line)]">
            {list.data.map((n) => (
              <li key={n.id}>
                <button type="button" onClick={() => !n.read && run(() => api.markRead([n.id]))} className={cx("flex w-full items-start gap-3 px-4 py-3 text-left cursor-pointer hover:bg-[var(--panel-2)]/50", !n.read && "bg-iris/5")}>
                  <span className={cx("mt-1.5 size-2 shrink-0 rounded-full", n.read ? "bg-transparent" : "bg-iris")} aria-label={n.read ? undefined : "unread"} />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2 font-medium">{n.platform && <PlatformDot platform={n.platform} />}{n.title}</div>
                    {n.body && <div className="text-[13px] break-words text-[var(--muted)]">{n.body}</div>}
                  </div>
                  <span className="shrink-0 text-xs text-[var(--muted)]">{fmtRelative(n.created_at, t.lang)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </>
  );
}
