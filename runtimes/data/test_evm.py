"""Offline tests for bounded public RPC evidence collection."""

import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import evm


ENDPOINTS = ["https://first.example/rpc", "https://second.example/rpc"]
ADDRESS = "0x" + "ab" * 20
TOPIC = "0x" + "cd" * 32


def digest(number):
    return "0x" + f"{number:064x}"


def reply(result):
    return {"jsonrpc": "2.0", "id": 1, "result": result}


def header(number):
    return {"number": hex(number), "hash": digest(number), "parentHash": digest(number - 1),
            "timestamp": hex(1_700_000_000 + number), "receiptsRoot": digest(20),
            "transactionsRoot": digest(21), "stateRoot": digest(22),
            "transactions": [digest(number + 100), digest(number + 200)]}


def logs(number):
    return [{"blockHash": digest(number), "blockNumber": hex(number),
             "transactionHash": digest(number + offset), "transactionIndex": hex(index),
             "logIndex": hex(index + 4), "address": ADDRESS, "topics": [TOPIC, digest(80)],
             "data": "0x1234", "removed": False} for index, offset in enumerate([100, 200])]


class Response(io.BytesIO):
    def __init__(self, body, declared=True):
        super().__init__(body)
        self.status = 200
        self.headers = {"Content-Length": str(len(body))} if declared else {}
        self.consumed = 0

    def read1(self, size=-1):
        data = super().read1(size)
        self.consumed += len(data)
        return data


class Network:
    def __init__(self, mutate=None):
        self.requests = []
        self.responses = []
        self.mutate = mutate

    def open(self, request, timeout):
        payload = json.loads(request.data)
        assert payload["jsonrpc"] == "2.0" and type(payload["id"]) is int and payload["id"] == 1
        assert timeout == 30 and request.get_header("Accept-encoding") == "identity"
        self.requests.append((request.full_url, payload))
        method, params = payload["method"], payload["params"]
        if method == "eth_chainId":
            assert params == []
            value = reply("0x89")
        elif method == "eth_getBlockByNumber":
            assert params[1] is False
            value = reply(header(int(params[0], 16)))
        elif method == "eth_getLogs":
            assert set(params[0]) == {"blockHash", "address", "topics"}
            assert params[0]["address"] == [ADDRESS] and params[0]["topics"] == [TOPIC]
            value = reply(logs(int(params[0]["blockHash"], 16)))
        else:
            raise AssertionError("unexpected method")
        if self.mutate:
            self.mutate(request.full_url, method, value)
        response = Response(json.dumps(value).encode())
        self.responses.append(response)
        return response


class EvmTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.output = Path(self.directory.name) / "capture.json"

    def acquire(self, network, **kwargs):
        options = {"chain_id": 137, "endpoints": ENDPOINTS, "blocks": [12, 11, 11],
                   "addresses": [ADDRESS], "topic0": TOPIC, "output": self.output}
        options.update(kwargs)
        with patch.object(evm.OPENER, "open", network.open):
            return evm.download(**options)

    def test_happy_path_preserves_raw_responses_and_compares_only_canonical_fields(self):
        def optional_fields(endpoint, method, value):
            if endpoint == ENDPOINTS[1]:
                if method == "eth_getBlockByNumber":
                    value["result"]["providerExtension"] = "preserved"
                elif method == "eth_getLogs":
                    value["result"].reverse()
                    value["result"][0]["blockTimestamp"] = "0x123"

        network = Network(optional_fields)
        summary = self.acquire(network)
        content = self.output.read_bytes()
        archive = json.loads(content)
        self.assertEqual(set(archive), {"schema_version", "chain_id", "retrieved_at", "query", "observations"})
        self.assertEqual(archive["query"], {"blocks": [11, 12], "addresses": [ADDRESS], "topic0": TOPIC})
        self.assertEqual(archive["chain_id"], 137)
        self.assertEqual(archive["schema_version"], 1)
        self.assertTrue(archive["retrieved_at"].endswith("Z"))
        self.assertEqual(archive["observations"][0]["chain_id"], reply("0x89"))
        observed = archive["observations"][1]["blocks"][0]
        self.assertEqual(observed["header"]["result"]["providerExtension"], "preserved")
        self.assertEqual(observed["logs"]["result"][0]["blockTimestamp"], "0x123")
        self.assertTrue(observed["received_at"].endswith("Z"))
        self.assertEqual(summary["rows"], 4)
        self.assertEqual(summary["bytes"], len(content))
        self.assertEqual(summary["sha256"], hashlib.sha256(content).hexdigest())
        self.assertEqual(summary["downloaded_bytes"], sum(r.consumed for r in network.responses))
        self.assertEqual(len(network.requests), 10)
        self.assertEqual(list(self.output.parent.iterdir()), [self.output])

    def test_preflight_rejects_unbounded_selection_private_or_authenticated_urls(self):
        invalid = ["http://first.example", "https://user:secret@first.example", "https://first.example/?key=secret",
                   "https://first.example/#secret", "https://localhost", "https://127.0.0.1", "https://[::1]"]
        for url in invalid:
            with self.subTest(url=url), self.assertRaises(ValueError):
                evm.endpoint_host(url)
        for options in ({"endpoints": [ENDPOINTS[0], ENDPOINTS[0] + "/again"]},
                        {"endpoints": ENDPOINTS + [ENDPOINTS[0]]},
                        {"endpoints": [f"https://rpc{i}.example" for i in range(5)]},
                        {"endpoints": ENDPOINTS[:1]},
                        {"blocks": []}, {"blocks": list(range(4097))}, {"chain_id": True},
                        {"addresses": []}, {"addresses": [ADDRESS] * 257},
                        {"topic0": "0x01"}, {"max_bytes": 0}):
            network = Network()
            with self.subTest(options=list(options)), self.assertRaises(ValueError):
                self.acquire(network, **options)
            self.assertEqual(network.requests, [])
        self.assertEqual(evm.select_blocks(first=11, last=12), [11, 12])
        for args in ((None, 0, 4096), (None, 12, 11), ([11], None, 12), (None, 11, None)):
            with self.subTest(args=args), self.assertRaises(ValueError):
                evm.select_blocks(*args)
        self.assertFalse(self.output.exists())

    def test_byte_limits_are_enforced_before_publication(self):
        network = Network()
        budget = 1000
        with self.assertRaises(ValueError):
            self.acquire(network, max_bytes=budget)
        self.assertLessEqual(sum(r.consumed for r in network.responses), budget)
        self.assertFalse(self.output.exists())
        for declared in (True, False):
            response = Response(b"x" * 65, declared=declared)
            with self.subTest(declared=declared), patch.object(evm, "MAX_RESPONSE_BYTES", 64):
                with patch.object(evm.OPENER, "open", return_value=response), self.assertRaises(ValueError):
                    evm.rpc_call(ENDPOINTS[0], "eth_chainId", [], 1000)
                self.assertLessEqual(response.consumed, 64)
        response = Response(json.dumps(reply("0x89")).encode())
        response.headers["Content-Length"] = str(len(response.getvalue()) + 1)
        with patch.object(evm.OPENER, "open", return_value=response), self.assertRaises(ValueError):
            evm.rpc_call(ENDPOINTS[0], "eth_chainId", [], 1000)

    def test_corrupt_json_and_rpc_errors_are_rejected(self):
        bodies = [b"{", b'{"jsonrpc":"2.0","id":1,"id":1,"result":"0x89"}',
                  b'{"jsonrpc":"2.0","id":1,"result":NaN}', json.dumps(reply(None)).encode(),
                  b'{"jsonrpc":"2.0","id":true,"result":"0x89"}',
                  b'{"jsonrpc":"2.0","id":1,"error":{"message":"SECRET"}}']
        for body in bodies:
            with self.subTest(body=body[:20]), patch.object(evm.OPENER, "open", return_value=Response(body)) as opened:
                with self.assertRaises(ValueError):
                    evm.download(137, ENDPOINTS, [11], [ADDRESS], TOPIC, self.output)
                self.assertEqual(opened.call_count, 1)
            self.assertFalse(self.output.exists())

    def test_connection_failure_retries_once_then_succeeds_with_same_budget(self):
        body = json.dumps(reply("0x89")).encode()
        for budget in (len(body), len(body) - 1):
            outcomes = [evm.urllib.error.URLError("SECRET connection failure"), Response(body)]
            with self.subTest(budget=budget), patch.object(evm.OPENER, "open", side_effect=outcomes) as opened:
                with patch.object(evm.time, "sleep") as sleep:
                    if budget == len(body):
                        value, size = evm.rpc_call(ENDPOINTS[0], "eth_chainId", [], budget)
                        self.assertEqual(value, reply("0x89"))
                        self.assertEqual(size, len(body))
                    else:
                        with self.assertRaises(ValueError):
                            evm.rpc_call(ENDPOINTS[0], "eth_chainId", [], budget)
                    sleep.assert_called_once_with(1)
                self.assertEqual(opened.call_count, 2)
                self.assertTrue(all(call.kwargs["timeout"] == 30 for call in opened.call_args_list))

    def test_connection_retries_stop_after_three_attempts(self):
        for error in (evm.urllib.error.URLError, TimeoutError, ConnectionError):
            with self.subTest(error=error.__name__), patch.object(evm.OPENER, "open", side_effect=error("SECRET")) as opened:
                with patch.object(evm.time, "sleep") as sleep, self.assertRaises(error):
                    evm.rpc_call(ENDPOINTS[0], "eth_chainId", [], 1000)
                self.assertEqual(opened.call_count, 3)
                self.assertEqual([call.args for call in sleep.call_args_list], [(1,), (2,)])

    def test_http_errors_are_never_retried(self):
        for status in (402, 403, 429, 500, 503):
            error = evm.urllib.error.HTTPError(ENDPOINTS[0], status, "SECRET", {}, None)
            with self.subTest(status=status), patch.object(evm.OPENER, "open", side_effect=error) as opened:
                with patch.object(evm.time, "sleep") as sleep, self.assertRaises(evm.urllib.error.HTTPError):
                    evm.rpc_call(ENDPOINTS[0], "eth_chainId", [], 1000)
                opened.assert_called_once()
                sleep.assert_not_called()

    def test_partial_body_failure_is_never_retried(self):
        class InterruptedResponse(Response):
            def read1(self, size=-1):
                if self.consumed:
                    raise evm.urllib.error.URLError("SECRET after partial body")
                return super().read1(min(size, 10))

        response = InterruptedResponse(json.dumps(reply("0x89")).encode())
        with patch.object(evm.OPENER, "open", return_value=response) as opened:
            with patch.object(evm.time, "sleep") as sleep, self.assertRaises(evm.urllib.error.URLError):
                evm.download(137, ENDPOINTS, [11], [ADDRESS], TOPIC, self.output)
            opened.assert_called_once()
            sleep.assert_not_called()
        self.assertEqual(response.consumed, 10)
        self.assertFalse(self.output.exists())

    def test_log_identity_removed_duplicates_and_order_fail_closed(self):
        def duplicate(value):
            value.append(copy.deepcopy(value[0]))

        def wrong_order(value):
            value[0]["logIndex"], value[1]["logIndex"] = value[1]["logIndex"], value[0]["logIndex"]

        mutations = [duplicate, wrong_order]
        mutations += [lambda value, k=key, v=val: value[0].__setitem__(k, v) for key, val in [
            ("removed", True), ("blockHash", digest(9)), ("blockNumber", "0xc"),
            ("transactionIndex", "0x1"), ("transactionHash", digest(9)),
            ("address", "0x" + "ef" * 20), ("topics", [digest(9)]), ("data", "0x1")]]
        for mutation in mutations:
            def corrupt(_endpoint, method, value):
                if method == "eth_getLogs":
                    mutation(value["result"])
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                self.acquire(Network(corrupt), blocks=[11])
            self.assertFalse(self.output.exists())

    def test_provider_chain_header_and_log_disagreement_fail_closed(self):
        changes = [("eth_chainId", lambda value: value.__setitem__("result", "0x1")),
                   ("eth_getBlockByNumber", lambda value: value.__setitem__("result", None)),
                   ("eth_getBlockByNumber", lambda value: value["result"].__setitem__("receiptsRoot", digest(99))),
                   ("eth_getLogs", lambda value: value["result"].pop()),
                   ("eth_getLogs", lambda value: value["result"][0].__setitem__("data", "0x5678"))]
        for target, change in changes:
            def disagree(endpoint, method, value):
                if endpoint == ENDPOINTS[1] and method == target:
                    change(value)
            with self.subTest(target=target), self.assertRaises(ValueError):
                self.acquire(Network(disagree))
            self.assertFalse(self.output.exists())

    def test_block_chain_order_observation_time_and_zero_hashes_fail_closed(self):
        def wrong_parent(_endpoint, method, value):
            if method == "eth_getBlockByNumber" and value["result"]["number"] == "0xc":
                value["result"]["parentHash"] = digest(99)

        def decreasing_time(_endpoint, method, value):
            if method == "eth_getBlockByNumber" and value["result"]["number"] == "0xd":
                value["result"]["timestamp"] = "0x1"

        def zero_block(_endpoint, method, value):
            if method == "eth_getBlockByNumber":
                value["result"]["hash"] = digest(0)

        def zero_transaction(_endpoint, method, value):
            if method == "eth_getBlockByNumber":
                value["result"]["transactions"][0] = digest(0)
            elif method == "eth_getLogs":
                value["result"][0]["transactionHash"] = digest(0)

        for mutation, blocks in ((wrong_parent, [11, 12]), (decreasing_time, [11, 13]),
                                 (zero_block, [11]), (zero_transaction, [11])):
            with self.subTest(mutation=mutation.__name__), self.assertRaises(ValueError):
                self.acquire(Network(mutation), blocks=blocks)
            self.assertFalse(self.output.exists())
        with patch.object(evm, "now", return_value="2020-01-01T00:00:00Z"), self.assertRaises(ValueError):
            self.acquire(Network(), blocks=[11])
        self.assertFalse(self.output.exists())
        times = ["2026-01-01T00:00:00Z", "2026-01-01T00:00:01Z", "2026-01-01T00:00:00Z"]
        with patch.object(evm, "now", side_effect=times), self.assertRaises(ValueError):
            self.acquire(Network(), blocks=[11])
        self.assertFalse(self.output.exists())
        # Gaps do not imply a parent linkage; timestamps may be equal.
        def equal_time(_endpoint, method, value):
            if method == "eth_getBlockByNumber":
                value["result"]["timestamp"] = hex(1_700_000_011)
        with patch.object(evm, "now", return_value="2023-11-14T22:13:31Z"):
            self.assertEqual(self.acquire(Network(equal_time), blocks=[11, 13])["blocks"], 2)

    def test_existing_file_symlinks_and_publication_race_never_overwrite(self):
        self.output.write_text("preserve")
        network = Network()
        with self.assertRaises(ValueError):
            self.acquire(network)
        self.assertEqual(network.requests, [])
        self.assertEqual(self.output.read_text(), "preserve")
        self.output.unlink()
        self.output.symlink_to(self.output.parent / "absent.json")
        with self.assertRaises(ValueError):
            self.acquire(network)
        self.assertTrue(self.output.is_symlink())
        self.output.unlink()

        def competing_writer(path, content):
            path.write_text("competing writer")
            evm.publish_bytes_original(path, content)

        with patch.object(evm, "publish_bytes_original", evm.publish_bytes, create=True):
            with patch.object(evm, "publish_bytes", side_effect=competing_writer), self.assertRaises(FileExistsError):
                self.acquire(Network())
        self.assertEqual(self.output.read_text(), "competing writer")
        self.assertEqual(list(self.output.parent.iterdir()), [self.output])

    def test_cli_suppresses_remote_error_body_and_exception_details(self):
        argv = ["--chain-id", "137", "--rpc", ENDPOINTS[0], "--rpc", ENDPOINTS[1],
                "--block", "11", "--address", ADDRESS, "--topic0", TOPIC, "--output", str(self.output)]
        for failure in (ValueError("SECRET response"), OSError("SECRET https://user:password@host"),
                        evm.urllib.error.URLError("SECRET connection failure"),
                        evm.urllib.error.HTTPError(ENDPOINTS[0], 403, "SECRET", {}, None)):
            stderr, stdout = io.StringIO(), io.StringIO()
            with patch.object(evm, "download", side_effect=failure), contextlib.redirect_stderr(stderr), contextlib.redirect_stdout(stdout):
                self.assertEqual(evm.main(argv), 1)
            self.assertEqual(stderr.getvalue(), "evm: acquisition failed; no archive published\n")
            self.assertEqual(stdout.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
