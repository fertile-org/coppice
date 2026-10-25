export interface RejectPreset {
  id: 'one_off' | 'too_vague' | 'wrong_scope' | 'duplicate';
  label: string;
  /** Human-readable audit string stored as rejectionReason. */
  reasonText: string;
}

export const REJECT_PRESETS: RejectPreset[] = [
  {
    id: 'one_off',
    label: 'One-off',
    reasonText: 'One-off for this ticket; not reusable next month.',
  },
  {
    id: 'too_vague',
    label: 'Too vague',
    reasonText: 'Too vague to apply reliably; needs clearer wording.',
  },
  {
    id: 'wrong_scope',
    label: 'Wrong scope',
    reasonText: 'Wrong scope; belongs elsewhere or is too broad/narrow.',
  },
  {
    id: 'duplicate',
    label: 'Duplicate',
    reasonText: 'Duplicate of existing approved knowledge.',
  },
];
