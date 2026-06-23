"use client";

import React, { useState, useCallback, useEffect } from "react";
import { DragDropContext, Droppable, Draggable, DropResult } from "@hello-pangea/dnd";
import {
  MoreHorizontal,
  Plus,
  Calendar,
  User,
  AlertCircle,
  CheckCircle2,
  Clock,
  MessageSquare,
  Paperclip,
  GripVertical
} from "lucide-react";
import { mapleApi } from "@/lib/api";
import { useTranslation } from "react-i18next";

export interface Task {
  id: string;
  title: string;
  description?: string;
  status: "todo" | "in-progress" | "review" | "done";
  priority: "low" | "medium" | "high";
  assignee?: {
    name: string;
    avatar?: string;
  };
  dueDate?: string;
  tags: string[];
  commentsCount: number;
  attachmentsCount: number;
}

export interface Column {
  id: string;
  title: string;
  tasks: Task[];
}

interface KanbanBoardProps {
  initialColumns?: Column[];
  onTaskMove?: (taskId: string, sourceColumn: string, targetColumn: string) => void;
  onTaskClick?: (task: Task) => void;
  filterQuery?: string;
}

const defaultColumns: Column[] = [
  {
    id: "todo",
    title: "待办",
    tasks: [
      {
        id: "task-1",
        title: "设计系统架构图",
        description: "完成整体架构设计和技术选型",
        status: "todo",
        priority: "high",
        assignee: { name: "张三", avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=zhang" },
        dueDate: "2026-05-28",
        tags: ["设计", "架构"],
        commentsCount: 3,
        attachmentsCount: 2,
      },
      {
        id: "task-2",
        title: "编写 API 接口文档",
        status: "todo",
        priority: "medium",
        assignee: { name: "李四", avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=li" },
        dueDate: "2026-05-30",
        tags: ["文档"],
        commentsCount: 1,
        attachmentsCount: 0,
      },
    ],
  },
  {
    id: "in-progress",
    title: "进行中",
    tasks: [
      {
        id: "task-3",
        title: "实现用户认证模块",
        description: "包含登录、注册、找回密码功能",
        status: "in-progress",
        priority: "high",
        assignee: { name: "王五", avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=wang" },
        dueDate: "2026-05-26",
        tags: ["开发", "后端"],
        commentsCount: 5,
        attachmentsCount: 1,
      },
      {
        id: "task-4",
        title: "前端组件库搭建",
        status: "in-progress",
        priority: "medium",
        assignee: { name: "赵六", avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=zhao" },
        tags: ["前端", "组件"],
        commentsCount: 2,
        attachmentsCount: 3,
      },
    ],
  },
  {
    id: "review",
    title: "审核中",
    tasks: [
      {
        id: "task-5",
        title: "代码审查：数据库设计",
        status: "review",
        priority: "high",
        assignee: { name: "钱七", avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=qian" },
        dueDate: "2026-05-25",
        tags: ["审查", "数据库"],
        commentsCount: 8,
        attachmentsCount: 1,
      },
    ],
  },
  {
    id: "done",
    title: "已完成",
    tasks: [
      {
        id: "task-6",
        title: "项目初始化",
        description: "完成项目脚手架搭建和依赖配置",
        status: "done",
        priority: "medium",
        assignee: { name: "孙八", avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=sun" },
        dueDate: "2026-05-20",
        tags: ["基础"],
        commentsCount: 0,
        attachmentsCount: 0,
      },
    ],
  },
];

const priorityConfig = {
  low: { color: "bg-emerald-100 text-emerald-700", icon: CheckCircle2 },
  medium: { color: "bg-amber-100 text-amber-700", icon: Clock },
  high: { color: "bg-rose-100 text-rose-700", icon: AlertCircle },
};

const tagColors = [
  "bg-blue-100 text-blue-700",
  "bg-purple-100 text-purple-700",
  "bg-pink-100 text-pink-700",
  "bg-teal-100 text-teal-700",
  "bg-orange-100 text-orange-700",
  "bg-cyan-100 text-cyan-700",
  "bg-indigo-100 text-indigo-700",
  "bg-rose-100 text-rose-700",
];

function tagColor(tag: string): string {
  let hash = 0;
  for (let i = 0; i < tag.length; i++) hash = ((hash << 5) - hash + tag.charCodeAt(i)) | 0;
  return tagColors[Math.abs(hash) % tagColors.length];
}

export function KanbanBoard({ initialColumns, onTaskMove, onTaskClick, filterQuery }: KanbanBoardProps) {
  const { t } = useTranslation();
  const [columns, setColumns] = useState<Column[]>(initialColumns || defaultColumns);
  const [draggingTaskId, setDraggingTaskId] = useState<string | null>(null);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [newTaskTitle, setNewTaskTitle] = useState("");
  const [newTaskDesc, setNewTaskDesc] = useState("");
  const [newTaskPriority, setNewTaskPriority] = useState<Task["priority"]>("medium");
  const [menuTaskId, setMenuTaskId] = useState<string | null>(null);
  const [editingTask, setEditingTask] = useState<Task | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [editDesc, setEditDesc] = useState("");
  const [editPriority, setEditPriority] = useState<Task["priority"]>("medium");
  const [editStatus, setEditStatus] = useState<Task["status"]>("todo");
  const [editAssignee, setEditAssignee] = useState("");
  const [editTags, setEditTags] = useState("");
  const [editDueDate, setEditDueDate] = useState("");
  const [detailTask, setDetailTask] = useState<Task | null>(null);
  const [attachments, setAttachments] = useState<Array<{ id: string; filename: string; size: number; content_type: string }>>([]);
  const [uploadingFile, setUploadingFile] = useState(false);

  const handleCreateTask = useCallback(async () => {
    if (!newTaskTitle.trim()) return;
    try {
      const res = await mapleApi<{ id: string }>("/api/board/tasks", {
        method: "POST",
        body: { title: newTaskTitle.trim(), description: newTaskDesc.trim() || undefined, status: "todo", priority: newTaskPriority, tags: [] },
      });
      const newTask: Task = {
        id: res.id,
        title: newTaskTitle.trim(),
        description: newTaskDesc.trim() || undefined,
        status: "todo",
        priority: newTaskPriority,
        tags: [],
        commentsCount: 0,
        attachmentsCount: 0,
      };
      setColumns((prev) =>
        prev.map((col) => (col.id === "todo" ? { ...col, tasks: [...col.tasks, newTask] } : col))
      );
      setShowCreateModal(false);
      setNewTaskTitle("");
      setNewTaskDesc("");
      setNewTaskPriority("medium");
    } catch {}
  }, [newTaskTitle, newTaskDesc, newTaskPriority]);

  const handleDeleteTask = useCallback(async (taskId: string) => {
    setColumns((prev) =>
      prev.map((col) => ({ ...col, tasks: col.tasks.filter((t) => t.id !== taskId) }))
    );
    setMenuTaskId(null);
    mapleApi(`/api/board/tasks/${taskId}`, { method: "DELETE" }).catch(() => {});
  }, []);

  const handleEditTask = useCallback(async () => {
    if (!editingTask || !editTitle.trim()) return;
    const updated = { ...editingTask, title: editTitle.trim(), description: editDesc.trim() || undefined };
    setColumns((prev) =>
      prev.map((col) => ({ ...col, tasks: col.tasks.map((t) => (t.id === editingTask.id ? updated : t)) }))
    );
    mapleApi(`/api/board/tasks/${editingTask.id}`, {
      method: "PUT",
      body: { title: editTitle.trim(), description: editDesc.trim() || undefined },
    }).catch(() => {});
    setEditingTask(null);
    setEditTitle("");
    setEditDesc("");
  }, [editingTask, editTitle, editDesc]);

  const openDetailModal = useCallback((task: Task) => {
    setDetailTask(task);
    setEditTitle(task.title);
    setEditDesc(task.description || "");
    setEditPriority(task.priority);
    setEditStatus(task.status);
    setEditAssignee(task.assignee?.name || "");
    setEditTags(task.tags.join(", "));
    setEditDueDate(task.dueDate || "");
    // Load attachments
    mapleApi<{ attachments: Array<{ id: string; filename: string; size: number; content_type: string }> }>(`/api/board/tasks/${task.id}/attachments`)
      .then((res) => setAttachments(res.attachments ?? []))
      .catch(() => setAttachments([]));
  }, []);

  const handleUploadAttachment = useCallback(async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files || files.length === 0 || !detailTask) return;
    setUploadingFile(true);
    try {
      const formData = new FormData();
      for (let i = 0; i < files.length; i++) formData.append("file", files[i]);
      const { token } = await import("@/lib/api").then((m) => m.getAuthState());
      const res = await fetch(`/api/maple/api/board/tasks/${detailTask.id}/attachments`, {
        method: "POST",
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        body: formData,
      });
      const data = await res.json();
      if (data.uploaded) setAttachments((prev) => [...data.uploaded, ...prev]);
    } catch {} finally { setUploadingFile(false); e.target.value = ""; }
  }, [detailTask]);

  const handleDeleteAttachment = useCallback(async (attId: string) => {
    try {
      await mapleApi(`/api/board/attachments/${attId}`, { method: "DELETE" });
      setAttachments((prev) => prev.filter((a) => a.id !== attId));
    } catch {}
  }, []);

  const handleSaveDetail = useCallback(async () => {
    if (!detailTask || !editTitle.trim()) return;
    const assigneeObj = editAssignee.trim() ? { name: editAssignee.trim() } : undefined;
    const tagsArr = editTags.split(",").map((t) => t.trim()).filter(Boolean);
    const updated: Task = { ...detailTask, title: editTitle.trim(), description: editDesc.trim() || undefined, priority: editPriority, status: editStatus, assignee: assigneeObj, tags: tagsArr, dueDate: editDueDate || undefined };
    // Move task to correct column if status changed
    setColumns((prev) => {
      const without = prev.map((col) => ({ ...col, tasks: col.tasks.filter((t) => t.id !== detailTask.id) }));
      return without.map((col) => (col.id === editStatus ? { ...col, tasks: [...col.tasks, updated] } : col));
    });
    mapleApi(`/api/board/tasks/${detailTask.id}`, {
      method: "PUT",
      body: { title: editTitle.trim(), description: editDesc.trim() || undefined, priority: editPriority, status: editStatus, assignee_name: assigneeObj?.name, tags: JSON.stringify(tagsArr), due_date: editDueDate || null },
    }).catch(() => {});
    setDetailTask(null);
  }, [detailTask, editTitle, editDesc, editPriority, editStatus, editAssignee, editTags]);

  useEffect(() => {
    if (initialColumns) return;
    mapleApi<{ tasks: Array<{ id: string; title: string; description?: string; status: string; priority: string; assignee?: { name: string; avatar?: string }; due_date?: string; tags: string[] }> }>("/api/board/tasks")
      .then((res) => {
        const tasks = res.tasks ?? [];
        if (tasks.length === 0) return;
        const statusMap: Record<string, string> = { todo: "todo", "in-progress": "in-progress", review: "review", done: "done" };
        const cols: Column[] = [
          { id: "todo", title: t("collab.kanban.columns.todo"), tasks: [] },
          { id: "in-progress", title: t("collab.kanban.columns.inProgress"), tasks: [] },
          { id: "review", title: t("collab.kanban.columns.review"), tasks: [] },
          { id: "done", title: t("collab.kanban.columns.done"), tasks: [] },
        ];
        for (const task of tasks) {
          const colIdx = cols.findIndex((c) => c.id === (statusMap[task.status] || "todo"));
          if (colIdx >= 0) {
            cols[colIdx].tasks.push({
              id: task.id,
              title: task.title,
              description: task.description,
              status: (statusMap[task.status] || "todo") as Task["status"],
              priority: (task.priority as Task["priority"]) || "medium",
              assignee: task.assignee,
              dueDate: task.due_date,
              tags: task.tags || [],
              commentsCount: 0,
              attachmentsCount: 0,
            });
          }
        }
        setColumns(cols);
      })
      .catch(() => {});
  }, [initialColumns]);

  const onDragStart = useCallback((start: { draggableId: string }) => {
    setDraggingTaskId(start.draggableId);
  }, []);

  const onDragEnd = useCallback((result: DropResult) => {
    setDraggingTaskId(null);

    if (!result.destination) return;

    const { source, destination } = result;
    
    if (source.droppableId === destination.droppableId && source.index === destination.index) {
      return;
    }

    const sourceColumn = columns.find((col) => col.id === source.droppableId);
    const destColumn = columns.find((col) => col.id === destination.droppableId);

    if (!sourceColumn || !destColumn) return;

    const sourceTasks = Array.from(sourceColumn.tasks);
    const [removed] = sourceTasks.splice(source.index, 1);

    if (source.droppableId === destination.droppableId) {
      sourceTasks.splice(destination.index, 0, removed);
      const newColumns = columns.map((col) =>
        col.id === source.droppableId ? { ...col, tasks: sourceTasks } : col
      );
      setColumns(newColumns);
    } else {
      const destTasks = Array.from(destColumn.tasks);
      const updatedTask = { ...removed, status: destination.droppableId as Task["status"] };
      destTasks.splice(destination.index, 0, updatedTask);
      
      const newColumns = columns.map((col) => {
        if (col.id === source.droppableId) return { ...col, tasks: sourceTasks };
        if (col.id === destination.droppableId) return { ...col, tasks: destTasks };
        return col;
      });

      setColumns(newColumns);
      onTaskMove?.(removed.id, source.droppableId, destination.droppableId);
      // Persist status change to backend
      mapleApi(`/api/board/tasks/${removed.id}`, {
        method: "PUT",
        body: { status: destination.droppableId },
      }).catch(() => {});
    }
  }, [columns, onTaskMove]);

  const getPriorityIcon = (priority: Task["priority"]) => {
    const Icon = priorityConfig[priority].icon;
    return <Icon className="w-3.5 h-3.5" />;
  };

  const getPriorityClass = (priority: Task["priority"]) => {
    return priorityConfig[priority].color;
  };

  const filteredColumns = filterQuery?.trim()
    ? columns.map((col) => ({
        ...col,
        tasks: col.tasks.filter((t) => {
          const q = filterQuery.toLowerCase();
          return t.title.toLowerCase().includes(q) || t.description?.toLowerCase().includes(q) || t.tags.some((tag) => tag.toLowerCase().includes(q));
        }),
      }))
    : columns;

  return (
    <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
      <div className="px-6 py-4 border-b border-border flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold">{t("collab.kanban.title")}</h2>
          <span className="px-2.5 py-1 bg-muted rounded-full text-xs font-medium text-muted-foreground">
            {t("collab.kanban.taskCount", { count: filteredColumns.reduce((acc, col) => acc + col.tasks.length, 0) })}
          </span>
        </div>
        <button
          onClick={() => setShowCreateModal(true)}
          className="flex items-center gap-1.5 px-3 py-1.5 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90 transition-opacity"
        >
          <Plus className="w-4 h-4" />
          {t("collab.kanban.newTask")}
        </button>
      </div>

      <DragDropContext onDragStart={onDragStart} onDragEnd={onDragEnd}>
        <div className="p-4 flex gap-4 overflow-x-auto min-h-[500px]">
          {filteredColumns.map((column) => (
            <div key={column.id} className="flex-shrink-0 w-80">
              <div className="bg-muted/50 rounded-lg p-3">
                <div className="flex items-center justify-between mb-3">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold">{column.title}</span>
                    <span className="px-1.5 py-0.5 bg-muted rounded text-xs text-muted-foreground">
                      {column.tasks.length}
                    </span>
                  </div>
                  <button className="p-1 text-muted-foreground hover:text-foreground rounded transition-colors">
                    <MoreHorizontal className="w-4 h-4" />
                  </button>
                </div>

                <Droppable droppableId={column.id}>
                  {(provided, snapshot) => (
                    <div
                      ref={provided.innerRef}
                      {...provided.droppableProps}
                      className={`space-y-2 min-h-[100px] ${
                        snapshot.isDraggingOver ? "bg-primary/5 rounded-lg" : ""
                      }`}
                    >
                      {column.tasks.map((task, index) => (
                        <Draggable key={task.id} draggableId={task.id} index={index}>
                          {(provided, snapshot) => (
                            <div
                              ref={provided.innerRef}
                              {...provided.draggableProps}
                              {...provided.dragHandleProps}
                              onClick={() => { openDetailModal(task); onTaskClick?.(task); }}
                              className={`bg-card p-4 rounded-lg border border-border cursor-pointer group transition-all ${
                                snapshot.isDragging
                                  ? "shadow-lg rotate-2 scale-105"
                                  : "hover:shadow-md hover:border-primary/20"
                              } ${draggingTaskId === task.id ? "opacity-50" : ""}`}
                            >
                              <div className="flex items-start justify-between mb-2">
                                <div className="flex items-center gap-2">
                                  <div className={`p-1 rounded ${getPriorityClass(task.priority)}`}>
                                    {getPriorityIcon(task.priority)}
                                  </div>
                                  {task.tags.slice(0, 2).map((tag) => (
                                    <span key={tag} className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${tagColor(tag)}`}>
                                      {tag}
                                    </span>
                                  ))}
                                  {task.tags.length > 2 && (
                                    <span className="px-1 py-0.5 bg-muted rounded text-[10px] text-muted-foreground">+{task.tags.length - 2}</span>
                                  )}
                                </div>
                                <div className="relative">
                                  <button
                                    onClick={(e) => { e.stopPropagation(); setMenuTaskId(menuTaskId === task.id ? null : task.id); }}
                                    className="opacity-0 group-hover:opacity-100 p-1 text-muted-foreground hover:text-foreground transition-opacity"
                                  >
                                    <MoreHorizontal className="w-4 h-4" />
                                  </button>
                                  {menuTaskId === task.id && (
                                    <div className="absolute top-full right-0 mt-1 py-1 bg-card border border-border rounded-lg shadow-lg z-20 min-w-[100px]">
                                      <button
                                        onClick={(e) => { e.stopPropagation(); openDetailModal(task); setMenuTaskId(null); }}
                                        className="w-full text-left px-3 py-1.5 text-xs hover:bg-muted transition-colors"
                                      >编辑</button>
                                      <button
                                        onClick={(e) => { e.stopPropagation(); handleDeleteTask(task.id); }}
                                        className="w-full text-left px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors"
                                      >删除</button>
                                    </div>
                                  )}
                                </div>
                              </div>

                              <h4 className="font-medium text-sm mb-1 line-clamp-2">{task.title}</h4>
                              {task.description && (
                                <p className="text-xs text-muted-foreground line-clamp-2 mb-3">
                                  {task.description}
                                </p>
                              )}

                              {task.assignee && (
                                <div className="flex items-center justify-between mt-3">
                                  <div className="flex items-center gap-2">
                                    {task.assignee.avatar ? (
                                      <img
                                        src={task.assignee.avatar}
                                        alt={task.assignee.name}
                                        className="w-6 h-6 rounded-full bg-muted"
                                      />
                                    ) : (
                                      <div className="w-6 h-6 rounded-full bg-primary/10 flex items-center justify-center text-xs font-medium">
                                        {task.assignee.name.charAt(0)}
                                      </div>
                                    )}
                                    <span className="text-xs text-muted-foreground truncate max-w-[80px]">
                                      {task.assignee.name}
                                    </span>
                                  </div>
                                  <div className="flex items-center gap-2 text-muted-foreground">
                                    {task.commentsCount > 0 && (
                                      <div className="flex items-center gap-0.5 text-xs">
                                        <MessageSquare className="w-3 h-3" />
                                        <span>{task.commentsCount}</span>
                                      </div>
                                    )}
                                    {task.attachmentsCount > 0 && (
                                      <div className="flex items-center gap-0.5 text-xs">
                                        <Paperclip className="w-3 h-3" />
                                        <span>{task.attachmentsCount}</span>
                                      </div>
                                    )}
                                  </div>
                                </div>
                              )}

                              {task.dueDate && (
                                <div className="flex items-center gap-1.5 mt-2 text-xs text-muted-foreground">
                                  <Calendar className="w-3 h-3" />
                                  <span>{task.dueDate}</span>
                                </div>
                              )}
                            </div>
                          )}
                        </Draggable>
                      ))}
                      {provided.placeholder}
                    </div>
                  )}
                </Droppable>

                <button
                  onClick={() => setShowCreateModal(true)}
                  className="w-full mt-2 flex items-center justify-center gap-1.5 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors"
                >
                  <Plus className="w-4 h-4" />
                  {t("collab.kanban.addCard")}
                </button>
              </div>
            </div>
          ))}
        </div>
      </DragDropContext>

      {/* Task Detail Modal */}
      {detailTask && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={() => setDetailTask(null)}>
          <div className="bg-card rounded-xl border border-border shadow-xl w-full max-w-lg p-6 max-h-[85vh] overflow-y-auto" onClick={(e) => e.stopPropagation()}>
            <h3 className="text-lg font-semibold mb-4">{t("collab.taskDetail.title")}</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.title")}</label>
                <input type="text" value={editTitle} onChange={(e) => setEditTitle(e.target.value)}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary" autoFocus />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.description")}</label>
                <textarea value={editDesc} onChange={(e) => setEditDesc(e.target.value)}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary resize-none" rows={3} />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.status")}</label>
                  <select value={editStatus} onChange={(e) => setEditStatus(e.target.value as Task["status"])}
                    className="w-full h-9 px-3 rounded border bg-background text-sm">
                    <option value="todo">待办</option>
                    <option value="in-progress">进行中</option>
                    <option value="review">审核中</option>
                    <option value="done">已完成</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.priority")}</label>
                  <div className="flex gap-2">
                    {(["low", "medium", "high"] as const).map((p) => (
                      <button key={p} onClick={() => setEditPriority(p)}
                        className={`flex-1 px-2 py-1.5 rounded-lg text-xs font-medium transition-colors ${priorityConfig[p].color} ${editPriority === p ? "ring-2 ring-primary" : ""}`}>
                        {p === "low" ? t("collab.taskDetail.priorityLow") : p === "medium" ? t("collab.taskDetail.priorityMedium") : t("collab.taskDetail.priorityHigh")}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.assigneeName")}</label>
                <input type="text" value={editAssignee} onChange={(e) => setEditAssignee(e.target.value)}
                  placeholder={t("collab.taskDetail.assigneePlaceholder")}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary" />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.tags")}</label>
                <input type="text" value={editTags} onChange={(e) => setEditTags(e.target.value)}
                  placeholder={t("collab.taskDetail.tagsPlaceholder")}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary" />
                {editTags && (
                  <div className="flex flex-wrap gap-1 mt-2">
                    {editTags.split(",").map((t) => t.trim()).filter(Boolean).map((tag, i) => (
                      <span key={i} className="px-2 py-0.5 bg-muted rounded text-xs text-muted-foreground">{tag}</span>
                    ))}
                  </div>
                )}
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.dueDateField")}</label>
                <input type="date" value={editDueDate} onChange={(e) => setEditDueDate(e.target.value)}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary" />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">附件</label>
                <div className="space-y-1.5">
                  {attachments.map((att) => (
                    <div key={att.id} className="flex items-center justify-between px-3 py-1.5 bg-muted rounded-lg text-xs">
                      <span className="truncate flex-1">{att.filename}</span>
                      <span className="text-muted-foreground mx-2">{(att.size / 1024).toFixed(1)}KB</span>
                      <button onClick={() => handleDeleteAttachment(att.id)} className="text-destructive hover:underline">删除</button>
                    </div>
                  ))}
                </div>
                <label className="mt-2 inline-flex items-center gap-1.5 px-3 py-1.5 bg-muted border border-border rounded-lg text-xs cursor-pointer hover:bg-muted/80 transition-colors">
                  {uploadingFile ? "上传中..." : "上传文件"}
                  <input type="file" className="hidden" multiple onChange={handleUploadAttachment} disabled={uploadingFile} />
                </label>
              </div>
            </div>
            <div className="flex justify-between mt-6">
              <button onClick={() => { handleDeleteTask(detailTask.id); setDetailTask(null); }}
                className="px-4 py-2 text-sm text-destructive hover:bg-destructive/10 border border-border rounded-lg transition-colors">{t("collab.taskDetail.deleteTask")}</button>
              <div className="flex gap-2">
                <button onClick={() => setDetailTask(null)} className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground border border-border rounded-lg hover:bg-muted transition-colors">{t("collab.taskDetail.cancel")}</button>
                <button onClick={handleSaveDetail} disabled={!editTitle.trim()}
                  className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition-opacity disabled:opacity-50">{t("collab.taskDetail.save")}</button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Create Task Modal */}
      {showCreateModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={() => setShowCreateModal(false)}>
          <div className="bg-card rounded-xl border border-border shadow-xl w-full max-w-md p-6" onClick={(e) => e.stopPropagation()}>
            <h3 className="text-lg font-semibold mb-4">{t("collab.kanban.newTask")}</h3>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.title")}</label>
                <input
                  type="text"
                  value={newTaskTitle}
                  onChange={(e) => setNewTaskTitle(e.target.value)}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder={t("collab.taskDetail.titlePlaceholder")}
                  autoFocus
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.description")}</label>
                <textarea
                  value={newTaskDesc}
                  onChange={(e) => setNewTaskDesc(e.target.value)}
                  className="w-full px-3 py-2 bg-muted border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary resize-none"
                  placeholder={t("collab.taskDetail.descPlaceholder")}
                  rows={3}
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1">{t("collab.taskDetail.priority")}</label>
                <div className="flex gap-2">
                  {(["low", "medium", "high"] as const).map((p) => (
                    <button
                      key={p}
                      onClick={() => setNewTaskPriority(p)}
                      className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${priorityConfig[p].color} ${newTaskPriority === p ? "ring-2 ring-primary" : ""}`}
                    >
                      {p === "low" ? t("collab.taskDetail.priorityLow") : p === "medium" ? t("collab.taskDetail.priorityMedium") : t("collab.taskDetail.priorityHigh")}
                    </button>
                  ))}
                </div>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-6">
              <button
                onClick={() => setShowCreateModal(false)}
                className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground border border-border rounded-lg hover:bg-muted transition-colors"
              >
                {t("collab.taskDetail.cancel")}
              </button>
              <button
                onClick={handleCreateTask}
                disabled={!newTaskTitle.trim()}
                className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition-opacity disabled:opacity-50"
              >
                {t("common.create")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default KanbanBoard;
