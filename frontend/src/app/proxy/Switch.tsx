export function IosSwitch({
  on,
  label,
  title,
  disabled,
  onChange,
}: {
  on: boolean;
  label?: string;
  title?: string;
  disabled?: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label
      className={`ios-switch${on ? " on" : ""}${disabled ? " disabled" : ""}`}
      title={title}
    >
      {label && <span>{label}</span>}
      <input
        type="checkbox"
        checked={on}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="ios-track">
        <span className="ios-knob" />
      </span>
    </label>
  );
}
