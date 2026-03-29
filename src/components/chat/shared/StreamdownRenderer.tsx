import { Streamdown } from 'streamdown';
import { code } from '@streamdown/code';
import { cjk } from '@streamdown/cjk';
import 'streamdown/styles.css';

interface StreamdownRendererProps {
  children: string;
  isStreaming?: boolean;
  mode?: 'streaming-stable' | 'final';
}

export function StreamdownRenderer({
  children,
  isStreaming: _isStreaming,
  mode = 'final',
}: StreamdownRendererProps): React.ReactElement {
  const streamingStable = mode === 'streaming-stable';

  return (
    <Streamdown
      plugins={{ code, cjk }}
      isAnimating={false}
      parseIncompleteMarkdown={streamingStable}
      className={streamingStable ? 'streaming-stable-markdown' : undefined}
    >
      {children}
    </Streamdown>
  );
}
