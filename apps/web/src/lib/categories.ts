import { Briefcase, GraduationCap, Heart, Home, ShoppingBag, Sparkles, type LucideIcon } from "lucide-react";
import type { Category } from "@tendly/contracts";

export type CategoryMeta = { key: Category; label: string; icon: LucideIcon; hint: string };

export const CATEGORIES: CategoryMeta[] = [
  { key: "home", label: "Home", icon: Home, hint: "Chores, upkeep, repairs" },
  { key: "errands", label: "Errands", icon: ShoppingBag, hint: "Shopping, pickups, bills" },
  { key: "people", label: "People", icon: Heart, hint: "Family, friends, plans" },
  { key: "school", label: "School", icon: GraduationCap, hint: "Classes, homework, exams" },
  { key: "work", label: "Work", icon: Briefcase, hint: "Jobs, clients, projects" },
  { key: "personal", label: "Personal", icon: Sparkles, hint: "Health, hobbies, self-care" },
];

export function categoryMeta(c: Category | null | undefined): CategoryMeta | undefined {
  return CATEGORIES.find((x) => x.key === c);
}
