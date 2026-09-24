import type { PerformanceClip } from "@wrela/model";

import { useId, useState } from "react";

type Event = PerformanceClip["events"][number];
type Props = {
  clip?: PerformanceClip;
  time: number;
  motion: { duration: number };
  editClip: (edit: (value: PerformanceClip) => void) => void;
  scrub: (seconds: number) => void;
};

function EventFields({
  event,
  duration,
  onEdit,
}: {
  event: Event;
  duration: number;
  onEdit: (edit: (event: Event) => void) => void;
}) {
  const controlId = useId();
  return (
    <fieldset>
      <legend>{event.id}</legend>
      <label className="row-control">
        Event identifier
        <input
          defaultValue={event.id}
          onBlur={(change) =>
            onEdit((value) => {
              value.id = change.target.value;
            })
          }
        />
      </label>
      <label className="row-control">
        Time (seconds)
        <input
          type="number"
          min={0}
          max={duration}
          step={0.01}
          value={event.time}
          onChange={(change) =>
            onEdit((value) => {
              value.time = Number(change.target.value);
            })
          }
        />
      </label>
      {Object.entries(event.payload).map(([key, payload]) => (
        <div key={key}>
          <label className="row-control">
            Parameter name
            <input
              defaultValue={key}
              onBlur={(change) =>
                onEdit((value) => {
                  const name = change.target.value;
                  if (name !== key && Object.hasOwn(value.payload, name))
                    throw new Error("That event parameter already exists");
                  value.payload = Object.fromEntries(
                    Object.entries(value.payload).map(([id, entry]) => [id === key ? name : id, entry]),
                  );
                })
              }
            />
          </label>
          <label className="row-control">
            Type
            <select
              value={typeof payload}
              onChange={(change) =>
                onEdit((value) => {
                  value.payload[key] =
                    change.target.value === "boolean"
                      ? false
                      : change.target.value === "number"
                        ? 0
                        : String(payload);
                })
              }
            >
              <option value="string">Text</option>
              <option value="number">Number</option>
              <option value="boolean">On / off</option>
            </select>
          </label>
          <label className="row-control" htmlFor={`${controlId}-${encodeURIComponent(key)}`}>
            Value
            {typeof payload === "boolean" ? (
              <input
                id={`${controlId}-${encodeURIComponent(key)}`}
                type="checkbox"
                checked={payload}
                onChange={(change) =>
                  onEdit((value) => {
                    value.payload[key] = change.target.checked;
                  })
                }
              />
            ) : (
              <input
                id={`${controlId}-${encodeURIComponent(key)}`}
                type={typeof payload === "number" ? "number" : "text"}
                value={payload}
                onChange={(change) =>
                  onEdit((value) => {
                    value.payload[key] =
                      typeof payload === "number" ? Number(change.target.value) : change.target.value;
                  })
                }
              />
            )}
          </label>
          <button
            type="button"
            onClick={() =>
              onEdit((value) => {
                delete value.payload[key];
              })
            }
          >
            Remove parameter
          </button>
        </div>
      ))}
      <button
        type="button"
        disabled={Object.keys(event.payload).length >= 16}
        onClick={() =>
          onEdit((value) => {
            let index = 1;
            while (Object.hasOwn(value.payload, `value-${index}`)) index++;
            value.payload[`value-${index}`] = "";
          })
        }
      >
        Add event parameter
      </button>
    </fieldset>
  );
}

export function PerformanceEventsPanel({ clip, time, motion, editClip, scrub }: Props) {
  const [eventName, setEventName] = useState("footstep");
  return (
    <details>
      <summary>Gameplay events ({clip?.events.length ?? 0})</summary>
      <input
        aria-label="Event name"
        value={eventName}
        onChange={(event) => setEventName(event.target.value)}
      />
      <button
        type="button"
        disabled={!eventName.trim()}
        onClick={() =>
          editClip((entry) => {
            let index = 1;
            while (entry.events.some((event) => event.id === `${eventName}-${index}`)) index++;
            entry.events.push({ id: `${eventName}-${index}`, time, payload: { cue: eventName } });
          })
        }
      >
        Add event at cursor
      </button>
      {clip?.events.map((event, index) => (
        <div key={event.id}>
          <button type="button" onClick={() => scrub(event.time)}>
            {event.id} · {event.time.toFixed(2)} s
          </button>
          <EventFields
            event={event}
            duration={motion.duration}
            onEdit={(edit) =>
              editClip((entry) => {
                edit(entry.events[index]);
              })
            }
          />
          <button
            type="button"
            onClick={() =>
              editClip((entry) => {
                entry.events = entry.events.filter((value) => value.id !== event.id);
              })
            }
          >
            Remove event
          </button>
        </div>
      ))}
    </details>
  );
}
