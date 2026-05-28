"use client";

import React, { useState, useRef, useEffect } from "react";
import {
  MessageSquare,
  Send,
  Paperclip,
  Smile,
  MoreHorizontal,
  Reply,
  ThumbsUp,
  Edit2,
  Trash2,
  AtSign,
  Hash,
  CornerDownRight,
  Check,
  X
} from "lucide-react";
import { mapleApi } from "@/lib/api";

export interface Comment {
  id: string;
  author: {
    name: string;
    avatar?: string;
    role?: string;
  };
  content: string;
  timestamp: string;
  likes: number;
  isLiked?: boolean;
  replies?: Comment[];
  isEditing?: boolean;
  attachments?: {
    name: string;
    type: string;
    size: string;
  }[];
  mentions?: string[];
}

interface CommentsProps {
  comments?: Comment[];
  taskId?: string;
  title?: string;
  placeholder?: string;
  onSendComment?: (content: string) => void;
  onReply?: (commentId: string, content: string) => void;
  onEdit?: (commentId: string, content: string) => void;
  onDelete?: (commentId: string) => void;
  onLike?: (commentId: string) => void;
}

const defaultComments: Comment[] = [
  {
    id: "comment-1",
    author: {
      name: "张三",
      avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=zhang",
      role: "架构师",
    },
    content: "这个任务的设计思路需要再讨论一下，我建议采用微服务架构，这样可以更好地支持后续的扩展。",
    timestamp: "10分钟前",
    likes: 3,
    isLiked: true,
    mentions: ["李四", "王五"],
    replies: [
      {
        id: "reply-1",
        author: {
          name: "李四",
          avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=li",
          role: "前端开发",
        },
        content: "同意！我这边已经开始调研相关的技术方案了。",
        timestamp: "5分钟前",
        likes: 1,
      },
    ],
  },
  {
    id: "comment-2",
    author: {
      name: "王五",
      avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=wang",
      role: "后端开发",
    },
    content: "关于用户认证模块的实现，我打算使用 JWT + Redis 的方案，大家觉得如何？",
    timestamp: "30分钟前",
    likes: 2,
    attachments: [
      { name: "auth-design.pdf", type: "pdf", size: "2.3 MB" },
      { name: "api-spec.yaml", type: "yaml", size: "15 KB" },
    ],
  },
  {
    id: "comment-3",
    author: {
      name: "赵六",
      avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=zhao",
      role: "UI 设计师",
    },
    content: "设计稿已更新，请大家查看最新版本。主要调整了配色方案和按钮样式。",
    timestamp: "1小时前",
    likes: 5,
  },
];

const getFileIcon = (type: string) => {
  switch (type) {
    case "pdf":
      return "bg-red-100 text-red-600";
    case "yaml":
    case "yml":
      return "bg-blue-100 text-blue-600";
    case "json":
      return "bg-yellow-100 text-yellow-600";
    case "md":
      return "bg-slate-100 text-slate-600";
    default:
      return "bg-muted text-muted-foreground";
  }
};

export function Comments({
  comments = defaultComments,
  taskId,
  title = "讨论区",
  placeholder = "发表你的看法...",
  onSendComment,
  onReply,
  onEdit,
  onDelete,
  onLike,
}: CommentsProps) {
  const [commentList, setCommentList] = useState<Comment[]>(comments);

  useEffect(() => {
    if (!taskId) return;
    mapleApi<{ comments: Array<{ id: string; parent_id?: string; author: { name: string; avatar?: string; role?: string }; content: string; likes: number; created_at: number }> }>(
      `/api/board/tasks/${taskId}/comments`
    ).then((res) => {
      const mapped: Comment[] = (res.comments ?? []).map((c) => ({
        id: c.id,
        author: c.author,
        content: c.content,
        timestamp: new Date(c.created_at * 1000).toLocaleString("zh-CN"),
        likes: c.likes,
        replies: [],
      }));
      setCommentList(mapped);
    }).catch(() => {});
  }, [taskId]);
  const [newComment, setNewComment] = useState("");
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [replyContent, setReplyContent] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editContent, setEditContent] = useState("");
  const [showEmoji, setShowEmoji] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const emojis = ["👍", "❤️", "🎉", "🤔", "👏", "🔥", "✅", "⚡", "🚀", "💯"];

  const handleSend = () => {
    if (!newComment.trim()) return;

    const comment: Comment = {
      id: `comment-${Date.now()}`,
      author: {
        name: "我",
        avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=me",
        role: "产品经理",
      },
      content: newComment,
      timestamp: "刚刚",
      likes: 0,
    };

    setCommentList([comment, ...commentList]);
    setNewComment("");
    onSendComment?.(newComment);

    if (taskId) {
      mapleApi("/api/board/comments", {
        method: "POST",
        body: { task_id: taskId, author_name: comment.author.name, author_avatar: comment.author.avatar, author_role: comment.author.role, content: comment.content },
      }).catch(() => {});
    }
  };

  const handleReply = (commentId: string) => {
    if (!replyContent.trim()) return;
    
    const reply: Comment = {
      id: `reply-${Date.now()}`,
      author: {
        name: "我",
        avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=me",
        role: "产品经理",
      },
      content: replyContent,
      timestamp: "刚刚",
      likes: 0,
    };
    
    setCommentList(
      commentList.map((c) =>
        c.id === commentId
          ? { ...c, replies: [...(c.replies || []), reply] }
          : c
      )
    );
    setReplyContent("");
    setReplyTo(null);
    onReply?.(commentId, replyContent);
  };

  const handleEdit = (commentId: string) => {
    if (!editContent.trim()) return;
    
    setCommentList(
      commentList.map((c) =>
        c.id === commentId ? { ...c, content: editContent } : c
      )
    );
    setEditingId(null);
    setEditContent("");
    onEdit?.(commentId, editContent);
  };

  const handleDelete = (commentId: string) => {
    setCommentList(commentList.filter((c) => c.id !== commentId));
    onDelete?.(commentId);
    mapleApi(`/api/board/comments/${commentId}`, { method: "DELETE" }).catch(() => {});
  };

  const handleLike = (commentId: string) => {
    setCommentList(
      commentList.map((c) =>
        c.id === commentId
          ? { ...c, likes: c.isLiked ? c.likes - 1 : c.likes + 1, isLiked: !c.isLiked }
          : c
      )
    );
    onLike?.(commentId);
    mapleApi(`/api/board/comments/${commentId}/like`, { method: "POST" }).catch(() => {});
  };

  const insertEmoji = (emoji: string) => {
    if (replyTo) {
      setReplyContent(replyContent + emoji);
    } else if (editingId) {
      setEditContent(editContent + emoji);
    } else {
      setNewComment(newComment + emoji);
    }
    setShowEmoji(false);
  };

  return (
    <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
      <div className="px-6 py-4 border-b border-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <h2 className="text-lg font-semibold">{title}</h2>
            <span className="px-2.5 py-1 bg-muted rounded-full text-xs font-medium text-muted-foreground">
              {commentList.length} 条讨论
            </span>
          </div>
          <button className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-muted transition-colors">
            <MoreHorizontal className="w-5 h-5" />
          </button>
        </div>
      </div>

      {/* New Comment Input */}
      <div className="p-4 border-b border-border">
        <div className="flex gap-3">
          <img
            src="https://api.dicebear.com/7.x/avataaars/svg?seed=me"
            alt="我"
            className="w-10 h-10 rounded-full bg-muted shrink-0"
          />
          <div className="flex-1">
            <div className="relative">
              <textarea
                ref={textareaRef}
                value={newComment}
                onChange={(e) => setNewComment(e.target.value)}
                placeholder={placeholder}
                rows={3}
                className="w-full px-4 py-3 bg-muted/50 border border-border rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary text-sm"
              />
              <div className="absolute bottom-2 right-2 flex items-center gap-1">
                <button
                  onClick={() => setShowEmoji(!showEmoji)}
                  className="p-1.5 text-muted-foreground hover:text-foreground rounded transition-colors"
                >
                  <Smile className="w-4 h-4" />
                </button>
              </div>
            </div>
            
            {showEmoji && (
              <div className="mt-2 p-2 bg-muted rounded-lg flex items-center gap-1">
                {emojis.map((emoji) => (
                  <button
                    key={emoji}
                    onClick={() => insertEmoji(emoji)}
                    className="w-8 h-8 flex items-center justify-center hover:bg-muted-foreground/10 rounded transition-colors text-lg"
                  >
                    {emoji}
                  </button>
                ))}
              </div>
            )}

            <div className="flex items-center justify-between mt-3">
              <div className="flex items-center gap-2">
                <button className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors">
                  <Paperclip className="w-4 h-4" />
                  附件
                </button>
                <button className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors">
                  <AtSign className="w-4 h-4" />
                  提及
                </button>
                <button className="flex items-center gap-1.5 px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors">
                  <Hash className="w-4 h-4" />
                  标签
                </button>
              </div>
              <button
                onClick={handleSend}
                disabled={!newComment.trim()}
                className="flex items-center gap-1.5 px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Send className="w-4 h-4" />
                发送
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Comments List */}
      <div className="p-4 space-y-4 max-h-[500px] overflow-y-auto">
        {commentList.map((comment) => (
          <div key={comment.id} className="group">
            <div className="flex gap-3">
              <img
                src={comment.author.avatar}
                alt={comment.author.name}
                className="w-10 h-10 rounded-full bg-muted shrink-0"
              />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="font-medium text-sm">{comment.author.name}</span>
                  <span className="text-xs text-muted-foreground">({comment.author.role})</span>
                  <span className="text-xs text-muted-foreground">{comment.timestamp}</span>
                </div>

                {editingId === comment.id ? (
                  <div className="space-y-2">
                    <textarea
                      value={editContent}
                      onChange={(e) => setEditContent(e.target.value)}
                      rows={3}
                      className="w-full px-3 py-2 bg-muted border border-border rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary text-sm"
                    />
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => handleEdit(comment.id)}
                        className="flex items-center gap-1 px-3 py-1 bg-primary text-primary-foreground rounded text-xs font-medium"
                      >
                        <Check className="w-3 h-3" />
                        保存
                      </button>
                      <button
                        onClick={() => {
                          setEditingId(null);
                          setEditContent("");
                        }}
                        className="flex items-center gap-1 px-3 py-1 border border-border rounded text-xs font-medium hover:bg-muted"
                      >
                        <X className="w-3 h-3" />
                        取消
                      </button>
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="text-sm text-foreground/90 whitespace-pre-wrap">
                      {comment.mentions && comment.mentions.length > 0 && (
                        <span className="text-primary">
                          {comment.mentions.map((m) => `@${m} `).join("")}
                        </span>
                      )}
                      {comment.content}
                    </div>

                    {/* Attachments */}
                    {comment.attachments && comment.attachments.length > 0 && (
                      <div className="flex flex-wrap gap-2 mt-2">
                        {comment.attachments.map((file, idx) => (
                          <div
                            key={idx}
                            className="flex items-center gap-2 px-3 py-2 bg-muted rounded-lg border border-border"
                          >
                            <div className={`w-8 h-8 rounded flex items-center justify-center text-xs font-bold ${getFileIcon(file.type)}`}>
                              {file.type.toUpperCase()}
                            </div>
                            <div>
                              <div className="text-xs font-medium">{file.name}</div>
                              <div className="text-[10px] text-muted-foreground">{file.size}</div>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Actions */}
                    <div className="flex items-center gap-4 mt-2">
                      <button
                        onClick={() => handleLike(comment.id)}
                        className={`flex items-center gap-1 text-xs transition-colors ${
                          comment.isLiked ? "text-primary" : "text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        <ThumbsUp className={`w-3.5 h-3.5 ${comment.isLiked ? "fill-current" : ""}`} />
                        <span>{comment.likes > 0 && comment.likes}</span>
                      </button>
                      <button
                        onClick={() => setReplyTo(replyTo === comment.id ? null : comment.id)}
                        className={`flex items-center gap-1 text-xs transition-colors ${
                          replyTo === comment.id ? "text-primary" : "text-muted-foreground hover:text-foreground"
                        }`}
                      >
                        <Reply className="w-3.5 h-3.5" />
                        回复
                      </button>
                      <button
                        onClick={() => {
                          setEditingId(comment.id);
                          setEditContent(comment.content);
                        }}
                        className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors opacity-0 group-hover:opacity-100"
                      >
                        <Edit2 className="w-3.5 h-3.5" />
                        编辑
                      </button>
                      <button
                        onClick={() => handleDelete(comment.id)}
                        className="flex items-center gap-1 text-xs text-muted-foreground hover:text-destructive transition-colors opacity-0 group-hover:opacity-100"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                        删除
                      </button>
                    </div>

                    {/* Reply Input */}
                    {replyTo === comment.id && (
                      <div className="mt-3 flex gap-2">
                        <img
                          src="https://api.dicebear.com/7.x/avataaars/svg?seed=me"
                          alt="我"
                          className="w-8 h-8 rounded-full bg-muted shrink-0"
                        />
                        <div className="flex-1">
                          <textarea
                            value={replyContent}
                            onChange={(e) => setReplyContent(e.target.value)}
                            placeholder={`回复 ${comment.author.name}...`}
                            rows={2}
                            className="w-full px-3 py-2 bg-muted border border-border rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary text-sm"
                          />
                          <div className="flex items-center justify-end gap-2 mt-2">
                            <button
                              onClick={() => setReplyTo(null)}
                              className="px-3 py-1 text-xs text-muted-foreground hover:text-foreground"
                            >
                              取消
                            </button>
                            <button
                              onClick={() => handleReply(comment.id)}
                              disabled={!replyContent.trim()}
                              className="flex items-center gap-1 px-3 py-1 bg-primary text-primary-foreground rounded text-xs font-medium disabled:opacity-50"
                            >
                              <Send className="w-3 h-3" />
                              回复
                            </button>
                          </div>
                        </div>
                      </div>
                    )}

                    {/* Replies */}
                    {comment.replies && comment.replies.length > 0 && (
                      <div className="mt-3 space-y-3 pl-4 border-l-2 border-border">
                        {comment.replies.map((reply) => (
                          <div key={reply.id} className="flex gap-2">
                            <img
                              src={reply.author.avatar}
                              alt={reply.author.name}
                              className="w-8 h-8 rounded-full bg-muted shrink-0"
                            />
                            <div className="flex-1">
                              <div className="flex items-center gap-2 mb-1">
                                <span className="font-medium text-sm">{reply.author.name}</span>
                                <span className="text-xs text-muted-foreground">
                                  ({reply.author.role})
                                </span>
                                <span className="text-xs text-muted-foreground">
                                  {reply.timestamp}
                                </span>
                              </div>
                              <div className="text-sm text-foreground/90">{reply.content}</div>
                              <div className="flex items-center gap-3 mt-1">
                                <button
                                  onClick={() => handleLike(reply.id)}
                                  className={`flex items-center gap-1 text-xs transition-colors ${
                                    reply.isLiked ? "text-primary" : "text-muted-foreground hover:text-foreground"
                                  }`}
                                >
                                  <ThumbsUp className={`w-3 h-3 ${reply.isLiked ? "fill-current" : ""}`} />
                                  <span>{reply.likes > 0 && reply.likes}</span>
                                </button>
                              </div>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default Comments;
