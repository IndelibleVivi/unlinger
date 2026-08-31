// make-menubar-icon.swift — regenerate the menu-bar template icon from the
// imagegen source art. Usage:
//   swift scripts/make-menubar-icon.swift AssetsSource/menubar-source.png Sources/UnlingerKit/Assets
//
// Menu-bar icons are template images: the shape is carried by the alpha
// channel and tinted by the system. The source art may come either way —
// black glyph on opaque white, or black glyph on transparency — so we decode
// premultiplied and take alpha = min(sourceAlpha, 255 - darkestChannel),
// which reduces to the right thing in both cases. Then crop to the content
// bbox with padding and directly emit the 18/36/54 px
// (@1x/@2x/@3x for an 18 pt glyph) production files. No derived master is
// written into the SwiftPM resource directory.

import CoreGraphics
import Foundation
import ImageIO

guard CommandLine.arguments.count == 3 else {
    fatalError("usage: make-menubar-icon.swift <source.png> <outdir>")
}
let sourceURL = URL(fileURLWithPath: CommandLine.arguments[1])
let outDir = URL(fileURLWithPath: CommandLine.arguments[2], isDirectory: true)

guard let imageSource = CGImageSourceCreateWithURL(sourceURL as CFURL, nil),
      let cgImage = CGImageSourceCreateImageAtIndex(imageSource, 0, nil)
else { fatalError("cannot decode \(sourceURL.path)") }

let width = cgImage.width
let height = cgImage.height
let buffer = UnsafeMutableBufferPointer<UInt8>.allocate(capacity: width * height * 4)
buffer.initialize(repeating: 0)
defer { buffer.deallocate() }
let colorSpace = CGColorSpaceCreateDeviceRGB()
guard let context = CGContext(
    data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
    bytesPerRow: width * 4, space: colorSpace,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { fatalError("cannot create bitmap context") }
context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))

var minX = width, minY = height, maxX = -1, maxY = -1
for y in 0..<height {
    for x in 0..<width {
        let i = (y * width + x) * 4
        let r = buffer[i], g = buffer[i + 1], b = buffer[i + 2], a = buffer[i + 3]
        let darkest = Int(min(r, min(g, b)))
        let alpha = UInt8(min(Int(a), 255 - darkest))
        buffer[i] = 0; buffer[i + 1] = 0; buffer[i + 2] = 0
        buffer[i + 3] = alpha
        if alpha > 24 {
            minX = min(minX, x); minY = min(minY, y)
            maxX = max(maxX, x); maxY = max(maxY, y)
        }
    }
}
guard maxX >= minX, maxY >= minY else { fatalError("no content found") }

// Content bbox + ~10% padding, centered on a square canvas.
let contentW = maxX - minX + 1
let contentH = maxY - minY + 1
let side = Int(Double(max(contentW, contentH)) * 1.2)
let centerX = (minX + maxX) / 2
let centerY = (minY + maxY) / 2
let cropX = max(0, min(centerX - side / 2, width - side))
let cropY = max(0, min(centerY - side / 2, height - side))
let cropSide = min(side, width - cropX, height - cropY)

guard let fullImage = context.makeImage(),
      let cropped = fullImage.cropping(to: CGRect(x: cropX, y: cropY, width: cropSide, height: cropSide))
else { fatalError("crop failed") }

func writeScaledPNG(_ source: CGImage, pixels: Int, name: String) throws {
    guard let scaledContext = CGContext(
        data: nil,
        width: pixels,
        height: pixels,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else { fatalError("cannot create \(pixels)x\(pixels) context") }
    scaledContext.interpolationQuality = .high
    scaledContext.draw(source, in: CGRect(x: 0, y: 0, width: pixels, height: pixels))
    guard let scaled = scaledContext.makeImage() else {
        fatalError("cannot render \(pixels)x\(pixels) icon")
    }

    let outputURL = outDir.appendingPathComponent(name)
    guard let destination = CGImageDestinationCreateWithURL(
        outputURL as CFURL,
        "public.png" as CFString,
        1,
        nil
    ) else { fatalError("cannot create destination for \(name)") }
    CGImageDestinationAddImage(destination, scaled, nil)
    guard CGImageDestinationFinalize(destination) else {
        fatalError("cannot write \(name)")
    }
    print("wrote \(outputURL.path) (\(pixels)x\(pixels))")
}

try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
try writeScaledPNG(cropped, pixels: 18, name: "menubar-icon.png")
try writeScaledPNG(cropped, pixels: 36, name: "menubar-icon@2x.png")
try writeScaledPNG(cropped, pixels: 54, name: "menubar-icon@3x.png")
