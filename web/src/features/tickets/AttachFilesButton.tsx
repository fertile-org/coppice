import { useRef } from 'react';
import { Paperclip } from 'lucide-react';
import { Button } from '../../components/ui/button';

interface AttachFilesButtonProps {
  disabled?: boolean;
  onFiles: (files: File[]) => void;
}

export function AttachFilesButton({ disabled, onFiles }: AttachFilesButtonProps) {
  const inputRef = useRef<HTMLInputElement>(null);

  return (
    <>
      <Button
        type="button"
        variant="secondary"
        size="sm"
        title="Attach files"
        disabled={disabled}
        onClick={() => inputRef.current?.click()}
      >
        <Paperclip className="size-4" aria-hidden="true" />
        Attach files
      </Button>
      <input
        ref={inputRef}
        type="file"
        multiple
        className="sr-only"
        tabIndex={-1}
        onChange={(event) => {
          if (event.target.files) onFiles(Array.from(event.target.files));
          event.target.value = '';
        }}
      />
    </>
  );
}
