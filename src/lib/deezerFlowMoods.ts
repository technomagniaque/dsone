/** Official Deezer Flow mood and genre `config_id` values accepted by
 *  `radio.getUserRadio` (`config_id` parameter). */

export interface DeezerFlowMood {
  id: string;
  label: string;
  subtitle: string;
  category: "mood" | "genre";
  gradient: string;
}

export const DEEZER_FLOW_MOODS: readonly DeezerFlowMood[] = [
  {
    id: "default",
    label: "Flow",
    subtitle: "Your personal mix, endless and fresh",
    category: "mood",
    gradient: "from-[#1f1c5c] via-[#6c2bd9] to-[#00c7f2]",
  },
  {
    id: "chill",
    label: "Chill",
    subtitle: "Laid-back tracks to unwind",
    category: "mood",
    gradient: "from-[#0f4c5c] via-[#1b7f8c] to-[#5fd3c7]",
  },
  {
    id: "motivation",
    label: "Motivation",
    subtitle: "High-energy boost for workouts & momentum",
    category: "mood",
    gradient: "from-[#b5179e] via-[#f72585] to-[#ff9e00]",
  },
  {
    id: "focus",
    label: "Focus",
    subtitle: "Deep concentration & minimal distractions",
    category: "mood",
    gradient: "from-[#1d3557] via-[#457b9d] to-[#8ecae6]",
  },
  {
    id: "party",
    label: "Party",
    subtitle: "Upbeat tracks to turn up the night",
    category: "mood",
    gradient: "from-[#7209b7] via-[#f72585] to-[#4cc9f0]",
  },
  {
    id: "melancholy",
    label: "Melancholy",
    subtitle: "Introspective & emotional picks",
    category: "mood",
    gradient: "from-[#22223b] via-[#4a4e69] to-[#9a8c98]",
  },
  {
    id: "you_and_me",
    label: "You & Me",
    subtitle: "Warm, intimate & romantic tracks",
    category: "mood",
    gradient: "from-[#780000] via-[#c1121f] to-[#ff758f]",
  },
  {
    id: "new_releases",
    label: "New Releases",
    subtitle: "Fresh drops tailored to your taste",
    category: "mood",
    gradient: "from-[#134e5e] via-[#2c7a7b] to-[#71b280]",
  },
  {
    id: "genre-rock",
    label: "Rock",
    subtitle: "Flow tuned to Rock",
    category: "genre",
    gradient: "from-[#590d22] via-[#a4133c] to-[#ff4d6d]",
  },
  {
    id: "genre-electronic",
    label: "Electronic",
    subtitle: "Flow tuned to Electronic",
    category: "genre",
    gradient: "from-[#10002b] via-[#5a189a] to-[#00bbf9]",
  },
  {
    id: "genre-jazz",
    label: "Jazz",
    subtitle: "Flow tuned to Jazz",
    category: "genre",
    gradient: "from-[#3d2645] via-[#832161] to-[#da4167]",
  },
  {
    id: "genre-classical",
    label: "Classical",
    subtitle: "Flow tuned to Classical",
    category: "genre",
    gradient: "from-[#2b2d42] via-[#5c677d] to-[#adb5bd]",
  },
  {
    id: "genre-pop",
    label: "Pop",
    subtitle: "Flow tuned to Pop",
    category: "genre",
    gradient: "from-[#ff006e] via-[#8338ec] to-[#3a86ff]",
  },
  {
    id: "genre-rap",
    label: "Rap / Hip-Hop",
    subtitle: "Flow tuned to Rap & Hip-Hop",
    category: "genre",
    gradient: "from-[#1b1b1e] via-[#495057] to-[#fca311]",
  },
  {
    id: "genre-metal",
    label: "Metal",
    subtitle: "Flow tuned to Metal",
    category: "genre",
    gradient: "from-[#111111] via-[#3d0000] to-[#950101]",
  },
  {
    id: "genre-lofi",
    label: "Lo-Fi",
    subtitle: "Flow tuned to Lo-Fi beats",
    category: "genre",
    gradient: "from-[#283618] via-[#606c38] to-[#dda15e]",
  },
  {
    id: "genre-ambient",
    label: "Ambient",
    subtitle: "Flow tuned to Ambient soundscapes",
    category: "genre",
    gradient: "from-[#03045e] via-[#0077b6] to-[#90e0ef]",
  },
  {
    id: "genre-soul",
    label: "Soul",
    subtitle: "Flow tuned to Soul & Funk",
    category: "genre",
    gradient: "from-[#6f1d1b] via-[#bb9457] to-[#ffe6a7]",
  },
  {
    id: "genre-chanson",
    label: "Chanson Française",
    subtitle: "Flow tuned to Chanson Française",
    category: "genre",
    gradient: "from-[#1d3557] via-[#e63946] to-[#f1faee]",
  },
  {
    id: "genre-oldschool",
    label: "Old School",
    subtitle: "Classic throwbacks from your library",
    category: "genre",
    gradient: "from-[#5f0f40] via-[#9a031e] to-[#fb8b24]",
  },
];

export function findFlowMood(id: string | undefined): DeezerFlowMood {
  return (
    DEEZER_FLOW_MOODS.find((m) => m.id === id) ?? DEEZER_FLOW_MOODS[0]
  );
}
