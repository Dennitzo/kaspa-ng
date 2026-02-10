interface VProgsHashLineProps {
  className?: string;
}

export default function VProgsHashLine({ className = "" }: VProgsHashLineProps) {
  return (
    <div className={`text-xs text-gray-400 whitespace-nowrap ${className}`.trim()}>
      Data computed by vProgs
    </div>
  );
}
