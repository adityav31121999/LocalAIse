import { ChevronLeftIcon, ChevronRightIcon } from "../ui/icons";
import { addMonths, formatDateKey, monthLabel, monthMatrix, toDateKey, weekdayLabels } from "../../util/calendar";

interface CalendarViewProps {
  month: Date;
  today: string;
  selectedDate: string | null;
  entryDates: Set<string>;
  onMonthChange(month: Date): void;
  onSelectDate(date: string): void;
}

export function CalendarView({
  month,
  today,
  selectedDate,
  entryDates,
  onMonthChange,
  onSelectDate,
}: CalendarViewProps) {
  const days = monthMatrix(month);

  return (
    <div className="cal">
      <div className="cal__head">
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          onClick={() => onMonthChange(addMonths(month, -1))}
          title="Previous month"
        >
          <ChevronLeftIcon size={14} />
          <span className="sr-only">Previous month</span>
        </button>
        <span className="cal__label">{monthLabel(month)}</span>
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          onClick={() => onMonthChange(addMonths(month, 1))}
          title="Next month"
        >
          <ChevronRightIcon size={14} />
          <span className="sr-only">Next month</span>
        </button>
      </div>

      <div className="cal__grid">
        {weekdayLabels().map((label) => (
          <abbr key={label} className="cal__weekday" title={label}>
            {label.slice(0, 1)}
          </abbr>
        ))}

        {days.map((day) => {
          const key = toDateKey(day);
          const hasEntry = entryDates.has(key);
          const modifiers = [
            day.getMonth() !== month.getMonth() ? "cal__day--outside" : "",
            key === today ? "cal__day--today" : "",
            key === selectedDate ? "cal__day--selected" : "",
            hasEntry ? "cal__day--has-entry" : "",
          ]
            .filter(Boolean)
            .join(" ");

          return (
            <button
              key={key}
              type="button"
              className={`cal__day ${modifiers}`}
              aria-current={key === selectedDate ? "true" : undefined}
              title={`${formatDateKey(key, { weekday: "long", day: "numeric", month: "long" })}${
                hasEntry ? " · entry" : ""
              }`}
              onClick={() => onSelectDate(key)}
            >
              {day.getDate()}
            </button>
          );
        })}
      </div>
    </div>
  );
}
