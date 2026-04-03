import { Box, IconButton, Typography } from '@mui/material';
import { X, FileText, FileSpreadsheet, Image, FileArchive, File, Presentation } from 'lucide-react';
import type { AttachedFile } from '@/stores/fileUploadStore';

const EXT_ICON_MAP: Record<string, React.ElementType> = {
  pdf: FileText,
  doc: FileText,
  docx: FileText,
  txt: FileText,
  xls: FileSpreadsheet,
  xlsx: FileSpreadsheet,
  csv: FileSpreadsheet,
  png: Image,
  jpg: Image,
  jpeg: Image,
  gif: Image,
  webp: Image,
  svg: Image,
  ppt: Presentation,
  pptx: Presentation,
  zip: FileArchive,
  rar: FileArchive,
  '7z': FileArchive,
  gz: FileArchive,
};

function getIcon(ext: string): React.ElementType {
  return EXT_ICON_MAP[ext] ?? File;
}

interface FileChipProps {
  file: AttachedFile;
  onRemove?: (id: string) => void;
}

export function FileChip({ file, onRemove }: FileChipProps): React.ReactElement {
  const Icon = getIcon(file.ext);

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
      <Icon size={14} color="#41484e" style={{ flexShrink: 0 }} />
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
