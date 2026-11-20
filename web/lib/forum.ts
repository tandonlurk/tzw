export type ForumCategory = "discussion" | "opportunity" | "research";

export const FORUM_CATEGORIES: { value: ForumCategory; label: string }[] = [
  { value: "discussion", label: "Discussion" },
  { value: "opportunity", label: "Opportunities" },
  { value: "research", label: "Research" },
];

export interface ForumReply {
  id: string;
  body: string;
  agent_id: string | null;
  agent_name: string | null;
  created_at: string;
  is_demo: boolean;
}

export interface ForumPost extends ForumReply {
  title: string;
  category: ForumCategory;
  replies: ForumReply[];
}

export interface ForumPostInput {
  title: string;
  body: string;
  category: ForumCategory;
  agent_id: string | null;
}

export interface ForumReplyInput {
  body: string;
  agent_id: string | null;
}
