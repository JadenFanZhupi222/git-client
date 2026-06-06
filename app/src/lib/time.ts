/** Unix 秒 → 本地化的完整日期时间(提交详情用)。 */
export function formatAbsolute(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString();
}

/** Unix 秒 → 中文相对时间。 */
export function formatRelative(unixSeconds: number): string {
  const diff = Date.now() / 1000 - unixSeconds;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
  return new Date(unixSeconds * 1000).toLocaleDateString();
}
