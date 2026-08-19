export function DetailMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded border border-line bg-elevated/60 px-2 py-1.5">
      <div className="text-[10px] uppercase tracking-wide text-fg-subtle">
        {label}
      </div>
      <div className="mt-0.5 truncate font-medium text-fg" title={value}>
        {value}
      </div>
    </div>
  );
}
