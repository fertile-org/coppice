import { useParams } from 'react-router-dom';

export function BoardPage() {
  const { projectId } = useParams<{ projectId: string }>();

  return (
    <div>
      <h1 className="font-display text-2xl font-semibold text-text-primary">
        Board
      </h1>
      <p className="mt-2 font-body text-text-secondary">
        Kanban board for project <span className="font-mono text-sm">{projectId}</span>{' '}
        ships in the next task.
      </p>
    </div>
  );
}
