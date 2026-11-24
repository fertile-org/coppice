import '@testing-library/jest-dom/vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { QuestionsForm } from './QuestionsForm';

const copy = {
  title: 'Questions from the agent',
  answerPlaceholder: 'Your answer',
  sendAnswers: 'Send answers',
  sending: 'Sending…',
};

const questions = [
  { question: 'Which screen?', choices: ['Inbox', 'Chat'] },
  { question: 'Keep the old endpoint?' },
];

describe('QuestionsForm', () => {
  it('keeps Send answers disabled until at least one answer is set', () => {
    render(<QuestionsForm questions={questions} copy={copy} onSubmit={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Send answers' })).toBeDisabled();
  });

  it('fills an answer from a chip and submits the answers array', async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<QuestionsForm questions={questions} copy={copy} onSubmit={onSubmit} />);
    fireEvent.click(screen.getByRole('button', { name: 'Inbox' }));
    const send = screen.getByRole('button', { name: 'Send answers' });
    expect(send).toBeEnabled();
    fireEvent.click(send);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(['Inbox', '']));
  });

  it('accepts a typed answer without chips', async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<QuestionsForm questions={questions} copy={copy} onSubmit={onSubmit} />);
    fireEvent.change(screen.getByLabelText('Keep the old endpoint?'), {
      target: { value: 'Yes' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Send answers' }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith(['', 'Yes']));
  });

  it('shows a read-only status without inputs', () => {
    render(
      <QuestionsForm
        questions={questions}
        copy={copy}
        readOnly
        statusLabel="Answered"
      />,
    );
    expect(screen.getByTestId('questions-form')).toHaveAttribute('data-readonly', 'true');
    expect(screen.getByText('Answered')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Send answers' })).toBeNull();
  });
});
