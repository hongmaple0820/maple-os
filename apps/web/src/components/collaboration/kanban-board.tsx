"use client";

import React, { useState, useCallback } from "react";
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

export function KanbanBoard({ initialColumns, onTaskMove, onTaskClick }: KanbanBoardProps) {
  const [columns, setColumns] = useState<Column[]>(initialColumns || defaultColumns);
  const [draggingTaskId, setDraggingTaskId] = useState<string | null>(null);

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
    }
  }, [columns, onTaskMove]);

  const getPriorityIcon = (priority: Task["priority"]) => {
    const Icon = priorityConfig[priority].icon;
    return <Icon className="w-3.5 h-3.5" />;
  };

  const getPriorityClass = (priority: Task["priority"]) => {
    return priorityConfig[priority].color;
  };

  return (
    <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
      <div className="px-6 py-4 border-b border-border flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-semibold">任务看板</h2>
          <span className="px-2.5 py-1 bg-muted rounded-full text-xs font-medium text-muted-foreground">
            {columns.reduce((acc, col) => acc + col.tasks.length, 0)} 个任务
          </span>
        </div>
        <button className="flex items-center gap-1.5 px-3 py-1.5 bg-primary text-primary-foreground rounded-md text-sm font-medium hover:opacity-90 transition-opacity">
          <Plus className="w-4 h-4" />
          新建任务
        </button>
      </div>

      <DragDropContext onDragStart={onDragStart} onDragEnd={onDragEnd}>
        <div className="p-4 flex gap-4 overflow-x-auto min-h-[500px]">
          {columns.map((column) => (
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
                              onClick={() => onTaskClick?.(task)}
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
                                  {task.tags.length > 0 && (
                                    <span className="px-1.5 py-0.5 bg-muted rounded text-[10px] text-muted-foreground">
                                      {task.tags[0]}
                                    </span>
                                  )}
                                </div>
                                <button className="opacity-0 group-hover:opacity-100 p-1 text-muted-foreground hover:text-foreground transition-opacity">
                                  <MoreHorizontal className="w-4 h-4" />
                                </button>
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

                <button className="w-full mt-2 flex items-center justify-center gap-1.5 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors">
                  <Plus className="w-4 h-4" />
                  添加卡片
                </button>
              </div>
            </div>
          ))}
        </div>
      </DragDropContext>
    </div>
  );
}

export default KanbanBoard;
