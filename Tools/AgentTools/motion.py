"""Bounded motion edits over Wrela's public native creature transaction protocol.

The native MotionPhrase implementation evaluates and compiles the edit. Contacts
remain source-identical. A preserved contact target is not proof of IK reach;
inspect the final pose report after committing. Bounds inspect 60 Hz candidate
samples and reject the transaction, without silently clamping the desired pose.
"""


def interval(transaction, phrase, *, start, end, fade_in, fade_out,
             adjustments, bounds=(), protected_times=()):
    """Adjust selected joints together while retaining source contacts/landings.

    adjustments=[dict(joint='body', offset=[0,-.2,0]),
                 dict(joint='head', delay=.2)]
    Channel bounds use joint/channel/minimum/maximum records. Positive delay
    follows the existing native movement later. Both fades must be positive.
    """
    values = []
    for adjustment in adjustments:
        unknown = set(adjustment) - {'joint', 'offset', 'rotation', 'delay'}
        if unknown:
            raise ValueError(f'Unknown motion adjustment fields: {sorted(unknown)}')
        values.append(dict(joint=adjustment['joint'],
                           offset=list(adjustment.get('offset', (0, 0, 0))),
                           rotation=list(adjustment.get('rotation', (0, 0, 0))),
                           delay=adjustment.get('delay', 0)))
    return transaction.operation('editInterval', key=phrase, edit=dict(
        start=start, end=end, fadeIn=fade_in, fadeOut=fade_out,
        adjustments=values, bounds=list(bounds), protectedTimes=list(protected_times)))
