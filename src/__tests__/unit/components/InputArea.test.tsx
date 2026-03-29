import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { InputArea } from '@/components/chat/InputArea';

describe('InputArea', () => {
  it('renders textarea with placeholder', () => {
    render(<InputArea value="" onChange={vi.fn()} onSend={vi.fn()} />);
    expect(screen.getByPlaceholderText('Type a message...')).toBeInTheDocument();
  });

  it('renders welcome variant placeholder', () => {
    render(<InputArea value="" onChange={vi.fn()} onSend={vi.fn()} variant="welcome" />);
    expect(screen.getByPlaceholderText('Ask anything or use @ and / for tools...')).toBeInTheDocument();
  });

  it('calls onChange when typing', async () => {
    const onChange = vi.fn();
    render(<InputArea value="" onChange={onChange} onSend={vi.fn()} />);
    const textarea = screen.getByPlaceholderText('Type a message...');
    fireEvent.change(textarea, { target: { value: 'hello' } });
    expect(onChange).toHaveBeenCalledWith('hello');
  });

  it('calls onSend on Enter key', async () => {
    const onSend = vi.fn();
    render(<InputArea value="test message" onChange={vi.fn()} onSend={onSend} />);
    const textarea = screen.getByPlaceholderText('Type a message...');
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false });
    expect(onSend).toHaveBeenCalledWith('test message');
  });

  it('does not call onSend on Shift+Enter', () => {
    const onSend = vi.fn();
    render(<InputArea value="test" onChange={vi.fn()} onSend={onSend} />);
    const textarea = screen.getByPlaceholderText('Type a message...');
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('does not call onSend when input is empty', () => {
    const onSend = vi.fn();
    render(<InputArea value="  " onChange={vi.fn()} onSend={onSend} />);
    const textarea = screen.getByPlaceholderText('Type a message...');
    fireEvent.keyDown(textarea, { key: 'Enter', shiftKey: false });
    expect(onSend).not.toHaveBeenCalled();
  });
});
