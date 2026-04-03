import { Box, IconButton, Typography } from '@mui/material';
import { X } from 'lucide-react';
import type { AttachedFile } from '@/stores/fileUploadStore';

import pdfIcon from '@/assets/file-icons/pdf.svg';
import excelIcon from '@/assets/file-icons/excel.svg';
import wordIcon from '@/assets/file-icons/word.svg';
import pptIcon from '@/assets/file-icons/ppt.svg';
import txtIcon from '@/assets/file-icons/txt.svg';
import imageIcon from '@/assets/file-icons/image.svg';
import archiveIcon from '@/assets/file-icons/archive.svg';
import unknownIcon from '@/assets/file-icons/unknown.svg';

const EXT_ICON_MAP: Record<string, string> = {
  pdf: pdfIcon,
  xls: excelIcon,
  xlsx: excelIcon,
  csv: excelIcon,
  doc: wordIcon,
  docx: wordIcon,
  ppt: pptIcon,
  pptx: pptIcon,
  txt: txtIcon,
  md: txtIcon,
  json: txtIcon,
  log: txtIcon,
  png: imageIcon,
  jpg: imageIcon,
  jpeg: imageIcon,
  gif: imageIcon,
  webp: imageIcon,
  svg: imageIcon,
  bmp: imageIcon,
  zip: archiveIcon,
  rar: archiveIcon,
  '7z': archiveIcon,
  gz: archiveIcon,
  tar: archiveIcon,
};

function getIcon(ext: string): string {
  return EXT_ICON_MAP[ext] ?? unknownIcon;
}

interface FileChipProps {
  file: Pick<AttachedFile, 'id' | 'name' | 'ext'>;
  onRemove?: (id: string) => void;
}

export function FileChip({ file, onRemove }: FileChipProps): React.ReactElement {
  const icon = getIcon(file.ext);

  return (
    <Box
      sx={{
        bgcolor: '#f2f4f6',
        border: '1px solid rgba(192,199,207,0.1)',
        borderRadius: 2,
        px: 1.5,
        py: 0.75,
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        maxWidth: 200,
      }}
    >
      <Box
        component="img"
        src={icon}
        alt=""
        sx={{ width: 15, height: 15, flexShrink: 0 }}
      />
      <Typography
        noWrap
        title={file.name}
        sx={{
          fontSize: 12,
          fontWeight: 500,
          color: '#41484e',
          maxWidth: 120,
        }}
      >
        {file.name}
      </Typography>
      {onRemove && (
        <IconButton
          size="small"
          onClick={() => onRemove(file.id)}
          sx={{ p: 0.25, ml: 0.5 }}
        >
          <X size={8} color="#41484e" />
        </IconButton>
      )}
    </Box>
  );
}
