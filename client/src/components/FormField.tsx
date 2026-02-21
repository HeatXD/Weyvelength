import { JSX } from "solid-js";

interface FormFieldProps {
  label: string;
  placeholder?: string;
  value: string;
  onInput: (v: string) => void;
  style?: JSX.CSSProperties;
}

export function FormField(props: FormFieldProps) {
  return (
    <div class="form-group" style={props.style}>
      <label>{props.label}</label>
      <input
        placeholder={props.placeholder}
        value={props.value}
        onInput={(e) => props.onInput(e.currentTarget.value)}
      />
    </div>
  );
}
