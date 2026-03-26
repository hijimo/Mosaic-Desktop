import { Streamdown } from 'streamdown';
import { code } from '@streamdown/code';
import { cjk } from '@streamdown/cjk';
import 'streamdown/styles.css';

interface StreamdownRendererProps {
  children: string;
  isStreaming?: boolean;
}

export function StreamdownRenderer({ children, isStreaming }: StreamdownRendererProps): React.ReactElement {
  return (
    <Streamdown
      plugins={{ code, cjk }}
      isAnimating={isStreaming}
    >
      {children}
    </Streamdown>
  );
}
