"""Converts the web's person segmenter (web/public/segment/selfie_segmenter.tflite)
to ONNX for Windows, and checks the result against MediaPipe itself.

The model has one operator TensorFlow Lite does not know: MediaPipe's custom
Convolution2DTransposeBias, the transposed convolution that brings the mask
up to 256 by 256, with its bias. It is rewritten as the built-in
TRANSPOSE_CONV, which has taken a bias since its third version, with the
same weights, padding and strides (its custom options are TfLite's
TfLiteTransposeConvParams: padding, stride across, stride down). Nothing
else is touched. tf2onnx then converts the rewritten model.

What is compared, on MediaPipe's own test portrait:
  MediaPipe's ImageSegmenter running the original model, as the web and
  Android run it, against TensorFlow Lite running the rewritten one, which
  tells whether the rewrite is the same network and what the input range is;
  and against ONNX Runtime running the converted one at the input Windows
  gives it.

tf2onnx also writes HARD_SWISH as ONNX's HardSwish, which is opset 14, in a
model it stamps 12; it is written back as x * HardSigmoid(x), which is its
definition, so that the model stays at 12, which Windows' own machine
learning (Windows.AI.MachineLearning) runs.

Run in a throwaway container, never on the machine itself, with
  /model     web/public/segment/selfie_segmenter.tflite
  /testdata  MediaPipe's portrait.jpg (storage.googleapis.com/mediapipe-assets/)
  /out       where these go:
    selfie_segmenter.onnx     the converted model (windows/Blur/)
    portrait-input.bin        the portrait as Windows hands it to the model (floats, NHWC)
    portrait-mediapipe.bin    MediaPipe's mask for the same picture (floats), which
                              the app's TOMOSHIBI_DEBUG_SEGMENT compares its own run with
    portrait-*.png            the masks, to look at
    report.txt                what was compared and how close it came
on python:3.11 with tensorflow 2.15.1, tf2onnx 1.16.1, onnx 1.16.2,
onnxruntime 1.18.1, mediapipe 0.10.14 (and opencv-python-headless in place
of the opencv it brings), numpy 1.26.4 and pillow:
  python convert.py 12

The run that made the model in this folder (2026-10-20) reported: TensorFlow
Lite on the rewritten model against MediaPipe on the original, largest
difference 0.00001 with the input in [0, 1] (0.90 in [-1, 1]); ONNX Runtime
against MediaPipe, largest difference 0.00001, IoU 1.0000 at 0.5; MediaPipe
on the whole 820x1024 portrait against the model on it stretched to the
square, IoU 0.9907, and kept in shape with a border, 0.9832.
"""
import subprocess
import sys

import mediapipe as mp
import numpy as np
import tensorflow as tf
from mediapipe.tasks import python as mp_tasks
from mediapipe.tasks.python import vision
from PIL import Image
from tensorflow.lite.python import schema_py_generated as schema
from tensorflow.lite.tools import flatbuffer_utils

ORIGINAL = "/model/selfie_segmenter.tflite"
REWRITTEN = "/out/selfie_segmenter-builtin.tflite"
OPSET = int(sys.argv[1]) if len(sys.argv) > 1 else 12
lines = []


def say(text):
    print(text)
    lines.append(text)


# The rewrite.
model = flatbuffer_utils.read_model(ORIGINAL)
graph = model.subgraphs[0]
custom = [i for i, code in enumerate(model.operatorCodes) if code.customCode == b"Convolution2DTransposeBias"]
transpose = schema.OperatorCodeT()
transpose.builtinCode = schema.BuiltinOperator.TRANSPOSE_CONV
transpose.deprecatedBuiltinCode = schema.BuiltinOperator.TRANSPOSE_CONV
transpose.version = 3
model.operatorCodes.append(transpose)
transpose_index = len(model.operatorCodes) - 1
rewritten = 0
for op in graph.operators:
    if op.opcodeIndex not in custom:
        continue
    padding, stride_w, stride_h = np.frombuffer(bytes(op.customOptions)[:12], "<i4").tolist()
    source, weights, bias = op.inputs
    out_shape = list(graph.tensors[op.outputs[0]].shape)
    buffer = schema.BufferT()
    buffer.data = np.array(out_shape, "<i4").tobytes()
    model.buffers.append(buffer)
    shape = schema.TensorT()
    shape.shape = [4]
    shape.type = schema.TensorType.INT32
    shape.buffer = len(model.buffers) - 1
    shape.name = b"segment/output_shape"
    graph.tensors.append(shape)
    options = schema.TransposeConvOptionsT()
    # TfLitePadding counts kTfLitePaddingSame as 1 and Valid as 2; the
    # schema's Padding counts SAME as 0 and VALID as 1.
    options.padding = {1: schema.Padding.SAME, 2: schema.Padding.VALID}[padding]
    options.strideW = stride_w
    options.strideH = stride_h
    options.fusedActivationFunction = schema.ActivationFunctionType.NONE
    op.opcodeIndex = transpose_index
    op.inputs = [len(graph.tensors) - 1, weights, source, bias]
    op.builtinOptionsType = schema.BuiltinOptions.TransposeConvOptions
    op.builtinOptions = options
    op.customOptions = None
    op.customOptionsFormat = 0
    rewritten += 1
    say(f"rewrote Convolution2DTransposeBias: padding {padding}, strides {stride_w}x{stride_h}, output {out_shape}")
flatbuffer_utils.write_model(model, REWRITTEN)
say(f"rewritten operators: {rewritten}")

interpreter = tf.lite.Interpreter(model_path=REWRITTEN)
interpreter.allocate_tensors()
inputs = interpreter.get_input_details()
outputs = interpreter.get_output_details()
_, height, width, _ = inputs[0]["shape"]


def run_tflite(x):
    interpreter.set_tensor(inputs[0]["index"], x)
    interpreter.invoke()
    return interpreter.get_tensor(outputs[0]["index"]).reshape(height, width).copy()


segmenter = vision.ImageSegmenter.create_from_options(vision.ImageSegmenterOptions(
    base_options=mp_tasks.BaseOptions(model_asset_path=ORIGINAL),
    running_mode=vision.RunningMode.IMAGE,
    output_confidence_masks=True,
    output_category_mask=False))


def run_mediapipe(rgb):
    result = segmenter.segment(mp.Image(image_format=mp.ImageFormat.SRGB, data=np.ascontiguousarray(rgb)))
    return result.confidence_masks[0].numpy_view().copy()


def iou(a, b):
    return np.logical_and(a > 0.5, b > 0.5).sum() / max(1, np.logical_or(a > 0.5, b > 0.5).sum())


portrait = Image.open("/testdata/portrait.jpg").convert("RGB")
# The portrait at the model's own size, so that MediaPipe resizes nothing and
# the two runs see the same pixels.
square = np.asarray(portrait.resize((width, height), Image.BILINEAR))
reference = run_mediapipe(square)
say(f"MediaPipe on the portrait at {width}x{height}: mask {reference.shape}, range {reference.min():.4f}..{reference.max():.4f}")
for name, x in (("[0, 1]", square.astype(np.float32) / 255.0), ("[-1, 1]", square.astype(np.float32) / 127.5 - 1.0)):
    mask = run_tflite(x[None, ...])
    say(f"TensorFlow Lite, rewritten, input in {name}, against MediaPipe: largest difference {np.abs(mask - reference).max():.5f}, mean {np.abs(mask - reference).mean():.6f}, IoU {iou(mask, reference):.4f}")
batch = (square.astype(np.float32) / 255.0)[None, ...]

# The whole portrait, which is taller than it is wide: how MediaPipe fits it
# to the square (stretched, or kept in shape with a border) decides how
# Windows must.
whole = np.asarray(portrait)
mediapipe_whole = run_mediapipe(whole)
stretched = np.asarray(Image.fromarray((run_tflite(batch) * 255).astype(np.uint8)).resize(portrait.size, Image.BILINEAR), np.float32) / 255.0
say(f"MediaPipe on the whole portrait ({whole.shape[1]}x{whole.shape[0]}) against the model on it stretched to the square: IoU {iou(mediapipe_whole, stretched):.4f}, mean difference {np.abs(mediapipe_whole - stretched).mean():.4f}")
side = max(portrait.size)
canvas = Image.new("RGB", (side, side))
offset = ((side - portrait.size[0]) // 2, (side - portrait.size[1]) // 2)
canvas.paste(portrait, offset)
boxed = run_tflite((np.asarray(canvas.resize((width, height), Image.BILINEAR), np.float32) / 255.0)[None, ...])
boxed_full = np.asarray(Image.fromarray((boxed * 255).astype(np.uint8)).resize((side, side), Image.BILINEAR), np.float32)[offset[1]:offset[1] + portrait.size[1], offset[0]:offset[0] + portrait.size[0]] / 255.0
say(f"... against the model on it kept in shape with a border: IoU {iou(mediapipe_whole, boxed_full):.4f}, mean difference {np.abs(mediapipe_whole - boxed_full).mean():.4f}")

result = subprocess.run([sys.executable, "-m", "tf2onnx.convert", "--tflite", REWRITTEN, "--output", "/out/selfie_segmenter.onnx", "--opset", str(OPSET)], capture_output=True, text=True)
if result.returncode != 0:
    print(result.stderr)
    sys.exit(1)
say("tf2onnx: " + [line for line in result.stderr.splitlines() if "Model inputs" in line or "Successfully" in line or "ONNX model is saved" in line][-1].split(" - ")[-1])

import onnx
import onnxruntime as ort
from onnx import helper

# tf2onnx writes TensorFlow Lite's HARD_SWISH as ONNX's HardSwish, which
# came in opset 14, and stamps the model 12 all the same, which ONNX Runtime
# rightly refuses. It is x times HardSigmoid(x) with alpha 1/6 and beta 1/2
# (the definition of HardSwish), and HardSigmoid is in every opset Windows'
# own machine learning runs; so it is written that way and the model stays
# at 12. The ai.onnx.ml import tf2onnx adds is used by nothing and goes.
converted = onnx.load("/out/selfie_segmenter.onnx")
nodes = []
swishes = 0
for node in converted.graph.node:
    if node.op_type != "HardSwish":
        nodes.append(node)
        continue
    gate = node.output[0] + "/gate"
    nodes.append(helper.make_node("HardSigmoid", [node.input[0]], [gate], name=node.name + "/gate", alpha=1.0 / 6.0, beta=0.5))
    nodes.append(helper.make_node("Mul", [node.input[0], gate], [node.output[0]], name=node.name))
    swishes += 1
del converted.graph.node[:]
converted.graph.node.extend(nodes)
kept = [o for o in converted.opset_import if (o.domain or "ai.onnx") == "ai.onnx"]
del converted.opset_import[:]
converted.opset_import.extend(kept)
onnx.checker.check_model(converted, full_check=True)
onnx.save(converted, "/out/selfie_segmenter.onnx")
say(f"HardSwish written as x * HardSigmoid(x): {swishes}")
say(f"onnx opset: {[(o.domain or 'ai.onnx', o.version) for o in converted.opset_import]}")
say(f"onnx operators: {sorted({n.op_type for n in converted.graph.node})}")
say(f"onnx inputs: {[(i.name, [d.dim_value or d.dim_param for d in i.type.tensor_type.shape.dim]) for i in converted.graph.input]}")
say(f"onnx outputs: {[(o.name, [d.dim_value or d.dim_param for d in o.type.tensor_type.shape.dim]) for o in converted.graph.output]}")
session = ort.InferenceSession("/out/selfie_segmenter.onnx", providers=["CPUExecutionProvider"])
onnx_mask = session.run(None, {session.get_inputs()[0].name: batch})[0].reshape(height, width)
say(f"ONNX Runtime against TensorFlow Lite (rewritten): largest difference {np.abs(onnx_mask - run_tflite(batch)).max():.6f}")
say(f"ONNX Runtime against MediaPipe (original): largest difference {np.abs(onnx_mask - reference).max():.5f}, mean {np.abs(onnx_mask - reference).mean():.6f}, IoU {iou(onnx_mask, reference):.4f}")
rng = np.random.default_rng(7)
for name, x in (("noise", rng.random((1, height, width, 3), dtype=np.float32)), ("grey", np.full((1, height, width, 3), 0.5, np.float32))):
    a = run_tflite(x)
    b = session.run(None, {session.get_inputs()[0].name: x})[0].reshape(height, width)
    say(f"ONNX Runtime against TensorFlow Lite on {name}: largest difference {np.abs(a - b).max():.6f}")

batch.astype("<f4").tofile("/out/portrait-input.bin")
reference.astype("<f4").tofile("/out/portrait-mediapipe.bin")
Image.fromarray((reference * 255).clip(0, 255).astype(np.uint8)).save("/out/portrait-mediapipe.png")
Image.fromarray((onnx_mask * 255).clip(0, 255).astype(np.uint8)).save("/out/portrait-onnx.png")
Image.fromarray((mediapipe_whole * 255).clip(0, 255).astype(np.uint8)).save("/out/portrait-mediapipe-whole.png")
open("/out/report.txt", "w").write("\n".join(lines) + "\n")
