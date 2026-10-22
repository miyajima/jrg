"""Score predeclared evidence units, including duplicated source alternatives."""


def score_units(documents, units):
    retained = []
    for unit in units:
        if any(
            document["url"] == unit["url"]
            and any(set(group) <= {block["id"] for block in document["blocks"]}
                    for group in unit["alternatives"])
            for document in documents
        ):
            retained.append(unit["id"])
    return retained
