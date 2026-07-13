import { useState, useRef, useEffect } from 'react'
import { ChevronDown, ChevronUp } from 'lucide-react'
import './ExpandableDescription.css'

interface ExpandableDescriptionProps {
  html: string
  maxHeight?: number
}

export function ExpandableDescription({ html, maxHeight = 200 }: ExpandableDescriptionProps) {
  const [isExpanded, setIsExpanded] = useState(false)
  const [needsExpansion, setNeedsExpansion] = useState(false)
  const contentRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (contentRef.current) {
      const fullHeight = contentRef.current.scrollHeight
      setNeedsExpansion(fullHeight > maxHeight)
    }
  }, [html, maxHeight])

  return (
    <div className="expandable-description">
      <div
        ref={contentRef}
        className={`description-html ${isExpanded ? 'expanded' : 'collapsed'}`}
        style={{
          maxHeight: isExpanded ? `${contentRef.current?.scrollHeight}px` : `${maxHeight}px`,
          overflow: 'hidden',
          transition: 'max-height 0.4s cubic-bezier(0.4, 0, 0.2, 1)',
        }}
        dangerouslySetInnerHTML={{ __html: html }}
      />
      
      {needsExpansion && (
        <button
          type="button"
          className="expand-toggle"
          onClick={() => setIsExpanded(!isExpanded)}
        >
          {isExpanded ? (
            <>
              Show less <ChevronUp size={16} />
            </>
          ) : (
            <>
              Read more <ChevronDown size={16} />
            </>
          )}
        </button>
      )}
    </div>
  )
}
