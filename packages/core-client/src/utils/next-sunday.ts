/**
 * The next Sunday at `hour`:00 local time, strictly after `from`. The default time of a service
 * in the event forms. It is plain date arithmetic that returns a value and keeps no state.
 */
export function nextSundayAt(hour: number, from: Date = new Date()): Date {
  const sunday = new Date(from);
  sunday.setHours(hour, 0, 0, 0);
  while (sunday.getDay() !== 0 || sunday <= from) sunday.setDate(sunday.getDate() + 1);
  return sunday;
}
