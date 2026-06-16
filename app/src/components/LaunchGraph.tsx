/** 启动屏背景:极淡、缓慢上行的「活的图谱」星座 —— 封面的签名意象,呼应产品本体(提交图谱)。
 *  纯装饰:absolute inset-0 / pointer-events-none / 低透明度(由 .launch-graph 控制),垫在玻璃内容卡之下。
 *  动效只用 transform(.launch-graph-drift,GPU 友好),无限无缝竖向漂移;prefers-reduced-motion 下静止。
 *  泳道用图谱可视化色(--lane-0 青绿 / --lane-1 蓝),空心节点用画布色「挖空」泳道——与历史图谱节点同口径。 */

// 纵向泳道(连续直线;纵移下视觉不变 → 作为稳定结构骨架)。x 取自 1200 宽 viewBox。
const LANES = [
  { x: 150, c: "var(--lane-0)" },
  { x: 340, c: "var(--lane-1)" },
  { x: 560, c: "var(--lane-0)" },
  { x: 800, c: "var(--lane-1)" },
  { x: 1010, c: "var(--lane-0)" },
  { x: 1150, c: "var(--lane-0)" },
];

// 一个高 360 的图谱单元:节点(实心/空心)。纵向平铺三份 → 漂移 -360 无缝循环。
const NODES = [
  { x: 150, y: 90, r: 5, hollow: false, c: "var(--lane-0)" },
  { x: 340, y: 200, r: 4.5, hollow: true, c: "var(--lane-1)" },
  { x: 560, y: 70, r: 5, hollow: false, c: "var(--lane-0)" },
  { x: 800, y: 280, r: 4.5, hollow: true, c: "var(--lane-1)" },
  { x: 1010, y: 150, r: 5, hollow: false, c: "var(--lane-0)" },
];

// 柔和的分支/合并曲线(相邻泳道之间弯一道),给「图谱」而非「平行线」的辨识度。
const CONNECTORS = [
  { d: "M340 200 C 340 120, 560 150, 560 70", c: "var(--lane-1)" },
  { d: "M1010 150 C 1010 230, 800 220, 800 280", c: "var(--lane-0)" },
];

const TILE_OFFSETS = [0, 360, 720];

export function LaunchGraph() {
  return (
    <svg
      aria-hidden
      className="launch-graph absolute inset-0 h-full w-full"
      viewBox="0 0 1200 720"
      preserveAspectRatio="xMidYMid slice"
      fill="none"
      style={{ pointerEvents: "none" }}
    >
      <g className="launch-graph-drift">
        {/* 纵向泳道:连续直线(超出上下边界以免漂移露白) */}
        {LANES.map((l, i) => (
          <line key={`lane-${i}`} x1={l.x} y1={-160} x2={l.x} y2={1240} stroke={l.c} strokeWidth={2} strokeLinecap="round" />
        ))}
        {/* 节点 + 连接线:每 360 平铺三份 → 无缝上行 */}
        {TILE_OFFSETS.map((off) => (
          <g key={`tile-${off}`} transform={`translate(0 ${off})`}>
            {CONNECTORS.map((c, i) => (
              <path key={`c-${i}`} d={c.d} stroke={c.c} strokeWidth={2} strokeLinecap="round" />
            ))}
            {NODES.map((n, i) =>
              n.hollow ? (
                <circle key={`n-${i}`} cx={n.x} cy={n.y} r={n.r} stroke={n.c} strokeWidth={2.2} fill="var(--color-canvas)" />
              ) : (
                <circle key={`n-${i}`} cx={n.x} cy={n.y} r={n.r} fill={n.c} />
              ),
            )}
          </g>
        ))}
      </g>
    </svg>
  );
}
