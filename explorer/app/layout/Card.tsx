import Spinner from "../Spinner";

interface CardProps {
  title?: string;
  value: string | number;
  subtext?: string;
  loading?: boolean;
  variant?: "default" | "analytics";
}

function Card({ title, value, subtext = "", loading, variant = "default" }: CardProps) {
  const baseClass =
    variant === "analytics"
      ? "grid grow rounded-3xl bg-gray-50 p-4"
      : "grid grow rounded-2xl border border-gray-100 p-4";
  return (
    <div className={baseClass}>
      <span className="text-sm sm:text-base">{title}</span>
      <span className="text-xl sm:text-2xl">
        {!loading ? value : (
          <span className="flex justify-center">
            <Spinner className="h-5 w-5" />
          </span>
        )}
      </span>
      <span className="text-gray-500">{subtext}</span>
    </div>
  );
}

export default Card;
