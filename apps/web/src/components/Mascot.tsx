/**
 * Pim — Tendly's original mascot: a round, pastel, penguin-ish imaginary
 * creature with little side flippers and a single antenna. Hand-drawn SVG;
 * no external image generation. Motion is subtle and turns off with
 * reduced-motion preferences.
 */
export type MascotPose = "happy" | "wave" | "sleepy" | "celebrate" | "think";

type Props = {
  pose?: MascotPose;
  size?: number;
  /** When set, the mascot is announced with this label; otherwise it is decorative. */
  label?: string;
  animated?: boolean;
};

export function Mascot({ pose = "happy", size = 96, label, animated = true }: Props) {
  const a11y = label ? { role: "img", "aria-label": label } : { "aria-hidden": true };
  const leftUp = pose === "celebrate";
  const rightUp = pose === "wave" || pose === "celebrate";
  return (
    <svg className="mascot" width={size} height={(size * 130) / 120} viewBox="0 0 120 130" {...a11y} focusable="false">
      <g className={animated ? "bob" : undefined}>
        {/* antenna */}
        <path d={pose === "think" ? "M60 28 Q58 16 66 9" : "M60 28 Q61 16 60 9"} fill="none" stroke="var(--mascot-line)" strokeWidth="2.5" strokeLinecap="round" />
        <circle className={animated ? "antenna-bulb" : undefined} cx={pose === "think" ? 67 : 60} cy="7" r="5.5" fill="var(--mascot-accent)" stroke="var(--mascot-line)" strokeWidth="2" />
        {/* flippers (behind body) */}
        <path
          d={leftUp ? "M26 66 Q10 52 12 38 Q22 46 30 58 Z" : "M25 68 Q9 82 14 98 Q24 90 30 78 Z"}
          fill="var(--mascot-body)"
          stroke="var(--mascot-line)"
          strokeWidth="2.5"
          strokeLinejoin="round"
        />
        <path
          d={rightUp ? "M94 66 Q110 52 108 38 Q98 46 90 58 Z" : "M95 68 Q111 82 106 98 Q96 90 90 78 Z"}
          fill="var(--mascot-body)"
          stroke="var(--mascot-line)"
          strokeWidth="2.5"
          strokeLinejoin="round"
        />
        {/* feet */}
        <ellipse cx="47" cy="118" rx="10" ry="5" fill="var(--mascot-accent)" stroke="var(--mascot-line)" strokeWidth="2" />
        <ellipse cx="73" cy="118" rx="10" ry="5" fill="var(--mascot-accent)" stroke="var(--mascot-line)" strokeWidth="2" />
        {/* body */}
        <path
          d="M60 26 C84 26 98 48 98 74 C98 100 82 116 60 116 C38 116 22 100 22 74 C22 48 36 26 60 26 Z"
          fill="var(--mascot-body)"
          stroke="var(--mascot-line)"
          strokeWidth="2.5"
        />
        {/* belly */}
        <path d="M60 58 C76 58 84 72 84 86 C84 100 74 110 60 110 C46 110 36 100 36 86 C36 72 44 58 60 58 Z" fill="var(--mascot-belly)" />
        {/* cheeks */}
        <ellipse cx="38" cy="70" rx="5.5" ry="3.2" fill="var(--mascot-accent)" opacity="0.7" />
        <ellipse cx="82" cy="70" rx="5.5" ry="3.2" fill="var(--mascot-accent)" opacity="0.7" />
        {/* eyes */}
        {pose === "sleepy" ? (
          <g fill="none" stroke="var(--mascot-line)" strokeWidth="2.5" strokeLinecap="round">
            <path d="M42 58 Q47 62 52 58" />
            <path d="M68 58 Q73 62 78 58" />
          </g>
        ) : pose === "celebrate" ? (
          <g fill="none" stroke="var(--mascot-line)" strokeWidth="2.5" strokeLinecap="round">
            <path d="M42 60 Q47 54 52 60" />
            <path d="M68 60 Q73 54 78 60" />
          </g>
        ) : (
          <g>
            <circle cx="47" cy={pose === "think" ? 56 : 58} r="5" fill="var(--mascot-line)" />
            <circle cx="73" cy={pose === "think" ? 56 : 58} r="5" fill="var(--mascot-line)" />
            <circle cx="48.6" cy={pose === "think" ? 54.4 : 56.4} r="1.6" fill="#fff" />
            <circle cx="74.6" cy={pose === "think" ? 54.4 : 56.4} r="1.6" fill="#fff" />
          </g>
        )}
        {/* beak-nub */}
        <path d="M55 66 Q60 62 65 66 Q60 72 55 66 Z" fill="var(--mascot-accent)" stroke="var(--mascot-line)" strokeWidth="1.8" strokeLinejoin="round" />
        {/* mouth */}
        {pose === "sleepy" ? (
          <circle cx="60" cy="77" r="2" fill="var(--mascot-line)" />
        ) : pose === "think" ? (
          <path d="M55 77 L65 76" stroke="var(--mascot-line)" strokeWidth="2.2" strokeLinecap="round" />
        ) : (
          <path d="M53 75 Q60 81 67 75" fill="none" stroke="var(--mascot-line)" strokeWidth="2.2" strokeLinecap="round" />
        )}
        {pose === "sleepy" && (
          <g fill="var(--mascot-line)" fontFamily="var(--font-round)" fontWeight="700">
            <text x="88" y="30" fontSize="12">z</text>
            <text x="97" y="20" fontSize="9">z</text>
          </g>
        )}
        {pose === "celebrate" && (
          <g fill="var(--mascot-accent)" stroke="var(--mascot-line)" strokeWidth="1.2">
            <path d="M14 22 l3 6 6 3 -6 3 -3 6 -3 -6 -6 -3 6 -3z" />
            <path d="M104 18 l2 4 4 2 -4 2 -2 4 -2 -4 -4 -2 4 -2z" />
          </g>
        )}
        {pose === "think" && (
          <g>
            <circle cx="96" cy="30" r="9" fill="var(--surface)" stroke="var(--mascot-line)" strokeWidth="1.8" />
            <text x="92.5" y="35" fontSize="13" fontWeight="700" fill="var(--mascot-line)">?</text>
          </g>
        )}
      </g>
    </svg>
  );
}
