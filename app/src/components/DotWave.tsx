// The result pill's dot waveform: columns of up to 5 dots, played columns in orange.

interface Props {
  values: number[];
  progress: number;
}

export function DotWave({ values, progress }: Props) {
  const cols = values.length || 28;
  const playedCols = Math.round(progress * cols);
  return (
    <div className="dot-wave" aria-hidden="true">
      {Array.from({ length: cols }, (_, i) => {
        const v = values[i] ?? 0;
        const n = 1 + Math.round(Math.min(1, v) * 4);
        return (
          <div className="col" key={i}>
            {Array.from({ length: 5 }, (_, j) => {
              const on = j >= 5 - n;
              const color = on ? (i < playedCols ? "#FF6A2B" : "#71717A") : "rgba(255,255,255,0.06)";
              return <span className="cell" key={j} style={{ background: color }} />;
            })}
          </div>
        );
      })}
    </div>
  );
}

export function formatTime(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
