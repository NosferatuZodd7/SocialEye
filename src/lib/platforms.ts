import type { Platform } from "./api";

export const PLATFORMS: Platform[] = ["instagram", "facebook", "youtube", "spotify"];

export const PLATFORM_META: Record<Platform, { label: string; color: string; provider: "google" | "meta" | "spotify"; audience: string; primary: string[] }> = {
  instagram: { label: "Instagram", color: "var(--color-instagram)", provider: "meta", audience: "followers", primary: ["reach", "follower_delta"] },
  facebook: { label: "Facebook", color: "var(--color-facebook)", provider: "meta", audience: "followers", primary: ["reach", "engagement", "page_views"] },
  youtube: { label: "YouTube", color: "var(--color-youtube)", provider: "google", audience: "subscribers", primary: ["views", "watch_minutes", "subscribers_gained"] },
  spotify: { label: "Spotify", color: "var(--color-spotify)", provider: "spotify", audience: "followers", primary: ["plays", "listeners"] },
};

/**
 * Metric names are platform-specific. Labels say exactly what is measured so that
 * e.g. YouTube views and Spotify plays are never presented as the same thing.
 */
export const METRIC_LABELS: Record<string, { en: string; pt: string }> = {
  subscribers: { en: "Subscribers", pt: "Subscritores" },
  followers: { en: "Followers", pt: "Seguidores" },
  following: { en: "Following", pt: "A seguir" },
  page_likes: { en: "Page likes", pt: "Gostos da página" },
  total_views: { en: "Lifetime views", pt: "Visualizações totais" },
  video_count: { en: "Videos", pt: "Vídeos" },
  media_count: { en: "Posts", pt: "Publicações" },
  episode_count: { en: "Episodes", pt: "Episódios" },
  views: { en: "Views", pt: "Visualizações" },
  watch_minutes: { en: "Watch time (min)", pt: "Tempo de visualização (min)" },
  avg_view_duration_secs: { en: "Avg. view duration (s)", pt: "Duração média (s)" },
  subscribers_gained: { en: "Subscribers gained", pt: "Subscritores ganhos" },
  subscribers_lost: { en: "Subscribers lost", pt: "Subscritores perdidos" },
  likes: { en: "Likes", pt: "Gostos" },
  comments: { en: "Comments", pt: "Comentários" },
  shares: { en: "Shares", pt: "Partilhas" },
  reactions: { en: "Reactions", pt: "Reações" },
  reach: { en: "Reach (accounts)", pt: "Alcance (contas)" },
  engagement: { en: "Post engagements", pt: "Interações" },
  page_views: { en: "Page views", pt: "Visitas à página" },
  new_followers: { en: "New followers", pt: "Novos seguidores" },
  follower_delta: { en: "Net follower change", pt: "Variação de seguidores" },
  plays: { en: "Plays", pt: "Reproduções" },
  starts: { en: "Starts", pt: "Inícios" },
  listeners: { en: "Listeners", pt: "Ouvintes" },
  audience_size: { en: "Audience size", pt: "Audiência" },
  consumption_hours: { en: "Consumption hours", pt: "Horas de consumo" },
  impressions: { en: "Impressions", pt: "Impressões" },
};

export const metricLabel = (m: string, lang: "en" | "pt" = "en") => METRIC_LABELS[m]?.[lang] ?? m.replace(/_/g, " ");

/** Metrics that are daily flows (summable over a range) vs. stock levels (take the latest). */
export const STOCK_METRICS = new Set(["subscribers", "followers", "following", "page_likes", "total_views", "video_count", "media_count", "episode_count", "audience_size"]);

export function summarize(metric: string, values: number[]): number | null {
  if (!values.length) return null;
  if (STOCK_METRICS.has(metric)) return values[values.length - 1];
  if (metric === "avg_view_duration_secs") return values.reduce((a, b) => a + b, 0) / values.length;
  return values.reduce((a, b) => a + b, 0);
}

export const KIND_LABELS: Record<string, { en: string; pt: string }> = {
  video: { en: "Video", pt: "Vídeo" }, short: { en: "Short (probable)", pt: "Short (provável)" }, live: { en: "Live", pt: "Direto" },
  post: { en: "Post", pt: "Publicação" }, photo: { en: "Photo", pt: "Foto" }, reel: { en: "Reel", pt: "Reel" },
  carousel: { en: "Carousel", pt: "Carrossel" }, story: { en: "Story", pt: "Story" }, episode: { en: "Episode", pt: "Episódio" },
};
